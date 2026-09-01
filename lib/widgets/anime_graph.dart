import 'dart:collection';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:auto_size_text/auto_size_text.dart';
import 'package:dailyanimelist/api/malapi.dart';
import 'package:dailyanimelist/constant.dart';
import 'package:dailyanimelist/enums.dart';
import 'package:dailyanimelist/generated/l10n.dart';
import 'package:dailyanimelist/screens/contentdetailedscreen.dart';
import 'package:dailyanimelist/screens/plainscreen.dart';
import 'package:dailyanimelist/widgets/home/animecard.dart';
import 'package:dailyanimelist/widgets/selectbottom.dart';
import 'package:dal_commons/commons.dart' as dal;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:graphview/GraphView.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';

enum _GraphOrderType {
  by_sequel,
  from_selected,
}

class AnimeGraphWidget extends StatefulWidget {
  final dal.AnimeGraph graph;
  final int id;
  final Map<int, dal.MyListStatus> statusMap;
  final List<Widget> actions;

  const AnimeGraphWidget({
    super.key,
    required this.graph,
    required this.id,
    required this.statusMap,
    required this.actions,
  });

  @override
  State<AnimeGraphWidget> createState() => _AnimeGraphWidgetState();
}

class _AnimeGraphWidgetState extends State<AnimeGraphWidget> {
  final Map<int, dal.GraphNode> _nodeMap = HashMap();
  final Set<int> _expandedIds = <int>{};
  final Set<String> _edgeKeys = <String>{};
  final GlobalKey _graphKey = GlobalKey();
  final GraphViewController _graphController = GraphViewController();

  late Graph _graph;
  late SugiyamaAlgorithm _algorithm;
  late int _selectedId;

  _GraphOrderType _graphOrderType = _GraphOrderType.from_selected;

  static const _sequelColor = Colors.green;
  static const _prequelColor = Colors.red;
  static const _otherColor = Colors.blue;

  @override
  void initState() {
    super.initState();
    _selectedId = widget.id;
    for (final node in widget.graph.nodes ?? const <dal.GraphNode>[]) {
      if (node.id != null) {
        _nodeMap[node.id!] = node;
      }
    }
    _buildGraph();
    _buildAlgorithm();
  }

  void _buildAlgorithm() {
    final configuration = SugiyamaConfiguration()
      ..orientation = SugiyamaConfiguration.ORIENTATION_TOP_BOTTOM
      ..nodeSeparation = 85
      ..levelSeparation = 125
      ..iterations = 18
      ..layeringStrategy = LayeringStrategy.longestPath
      ..crossMinimizationStrategy = CrossMinimizationStrategy.accumulatorTree
      ..coordinateAssignment = CoordinateAssignment.Average
      ..postStraighten = true
      ..addTriangleToEdge = true
      ..bendPointShape = CurvedBendPointShape(curveLength: 90);

    _algorithm = SugiyamaAlgorithm(configuration);
  }

  void _buildGraph() {
    _graph = Graph()..isTree = false;
    _edgeKeys.clear();

    // Add every node first so isolated related titles are still visible.
    for (final id in _nodeMap.keys) {
      _graph.addNode(Node.Id(id));
    }

    for (final edge in widget.graph.edges ?? const <dal.GraphEdge>[]) {
      final mapped = _mapEdge(edge);
      final source = mapped.source;
      final target = mapped.target;
      if (source == null || target == null) continue;
      if (!_nodeMap.containsKey(source) || !_nodeMap.containsKey(target)) continue;

      final key = '$source->$target:${mapped.relationType}';
      if (!_edgeKeys.add(key)) continue;

      _graph.addEdge(
        Node.Id(source),
        Node.Id(target),
        paint: Paint()
          ..color = _getColorByRelationType(mapped.relationType)
          ..strokeWidth = _getStrokeWidth(mapped.relationType)
          ..style = PaintingStyle.stroke
          ..strokeCap = StrokeCap.round,
      );
    }
  }

  dal.GraphEdge _mapEdge(dal.GraphEdge edge) {
    if (_graphOrderType == _GraphOrderType.by_sequel) {
      if (edge.relationType == dal.GRelationType.prequel) {
        return dal.GraphEdge(
          source: edge.target,
          target: edge.source,
          relationType: dal.GRelationType.sequel,
        );
      }

      if (edge.relationType != dal.GRelationType.sequel) {
        final chronological = _chronologicalEdge(edge);
        if (chronological != null) return chronological;
      }
    }
    return edge;
  }

  dal.GraphEdge? _chronologicalEdge(dal.GraphEdge edge) {
    final source = edge.source;
    final target = edge.target;
    if (source == null || target == null) return null;

    final first = _nodeMap[source]?.startSeason;
    final second = _nodeMap[target]?.startSeason;
    if (first?.year == null || second?.year == null) return edge;
    if (first?.season == null || second?.season == null) return edge;

    try {
      final firstDate = MalApi.getDateTimeForSeason(
        seasonMapInverse[dal.seasonValues.reverse[first!.season]]!,
        first.year!,
      );
      final secondDate = MalApi.getDateTimeForSeason(
        seasonMapInverse[dal.seasonValues.reverse[second!.season]]!,
        second.year!,
      );
      return firstDate.isBefore(secondDate)
          ? edge
          : dal.GraphEdge(
              source: target,
              target: source,
              relationType: edge.relationType,
            );
    } catch (_) {
      return edge;
    }
  }

  Color _getColorByRelationType(dal.GRelationType? relationType) {
    switch (relationType) {
      case dal.GRelationType.sequel:
        return _sequelColor;
      case dal.GRelationType.prequel:
        return _prequelColor;
      default:
        return _otherColor;
    }
  }

  double _getStrokeWidth(dal.GRelationType? relationType) {
    switch (relationType) {
      case dal.GRelationType.sequel:
      case dal.GRelationType.prequel:
        return 3.2;
      default:
        return 2.2;
    }
  }

  void _rebuildLayout() {
    _buildGraph();
    _buildAlgorithm();
    _graphController.forceRecalculation();
    if (mounted) setState(() {});
  }

  void _focusSelected({bool animated = true}) {
    _selectedId = _selectedId == 0 ? widget.id : _selectedId;
    final key = ValueKey(_selectedId);
    if (animated) {
      _graphController.animateToNode(key);
    } else {
      _graphController.jumpToNode(key);
    }
  }

  @override
  Widget build(BuildContext context) {
    return TitlebarScreen(
      Stack(
        children: [
          RepaintBoundary(
            key: _graphKey,
            child: GraphView.builder(
              graph: _graph,
              algorithm: _algorithm,
              controller: _graphController,
              animated: true,
              initialNode: ValueKey(widget.id),
              centerGraph: true,
              panAnimationDuration: const Duration(milliseconds: 450),
              toggleAnimationDuration: const Duration(milliseconds: 300),
              builder: (Node node) {
                final id = node.key?.value as int?;
                final data = id == null ? null : _nodeMap[id];
                return data == null
                    ? const SizedBox(width: 120, height: 120)
                    : _nodeWidget(data);
              },
            ),
          ),
          _bottomBar(),
        ],
      ),
      appbarTitle: '${S.current.Related} anime',
      autoIncludeSearch: false,
      actions: [
        SelectButton(
          popupText: S.current.Order_by,
          selectedOption: _graphTypeLabel,
          child: const Icon(Icons.swap_vert),
          options: _graphTypeOptions,
          onChanged: (value) {
            final index = _graphTypeOptions.indexOf(value);
            _graphOrderType = index == 0
                ? _GraphOrderType.from_selected
                : _GraphOrderType.by_sequel;
            _rebuildLayout();
            WidgetsBinding.instance.addPostFrameCallback((_) {
              _focusSelected(animated: false);
            });
          },
        ),
        ...widget.actions,
      ],
    );
  }

  List<String> get _graphTypeOptions => [
        S.current.Graph_Order_From_Selected,
        S.current.Graph_Order_By_Sequel,
      ];

  String get _graphTypeLabel => _graphOrderType == _GraphOrderType.from_selected
      ? S.current.Graph_Order_From_Selected
      : S.current.Graph_Order_By_Sequel;

  Widget _bottomBar() {
    return Positioned(
      left: 16,
      right: 16,
      bottom: 18,
      child: SafeArea(
        top: false,
        child: Row(
          children: [
            IconButton.filled(
              tooltip: 'Focus selected',
              onPressed: () => _focusSelected(),
              icon: const Icon(Icons.center_focus_strong),
            ),
            const Spacer(),
            IconButton.filledTonal(
              tooltip: 'Fit graph',
              onPressed: () => _graphController.zoomToFit(),
              icon: const Icon(Icons.fit_screen),
            ),
            const SizedBox(width: 8),
            IconButton.filledTonal(
              tooltip: 'Reset view',
              onPressed: () => _graphController.resetView(),
              icon: const Icon(Icons.refresh),
            ),
            const SizedBox(width: 8),
            IconButton.filledTonal(
              tooltip: 'Edge information',
              onPressed: _showEdgeInfo,
              icon: const Icon(Icons.info_outline),
            ),
            const SizedBox(width: 8),
            IconButton.filledTonal(
              tooltip: 'Share graph',
              onPressed: _captureAndSharePng,
              icon: const Icon(Icons.camera_alt_outlined),
            ),
          ],
        ),
      ),
    );
  }

  Widget _nodeWidget(dal.GraphNode node) {
    final expanded = _expandedIds.contains(node.id);
    final selected = _selectedId == node.id;
    final status = NodeStatusValue.fromListStatus(widget.statusMap[node.id]);

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        _cover(node, selected: selected, expanded: expanded, status: status),
        const SizedBox(height: 8),
        _titleCard(node, expanded: expanded),
      ],
    );
  }

  Widget _cover(
    dal.GraphNode node, {
    required bool selected,
    required bool expanded,
    required NodeStatusValue status,
  }) {
    final imageUrl = node.mainPicture?.large ?? node.mainPicture?.medium ?? '';
    final borderColor = status.color ?? Theme.of(context).colorScheme.outline;

    return GestureDetector(
      onTap: () => _toggleExpanded(node),
      onLongPress: () => _selectNode(node),
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 180),
        width: selected ? 152 : 144,
        height: selected ? 152 : 144,
        padding: const EdgeInsets.all(8),
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: Theme.of(context).cardColor,
          border: Border.all(
            color: selected ? Theme.of(context).colorScheme.primary : borderColor,
            width: selected ? 4 : 2.5,
          ),
          boxShadow: [
            if (selected || expanded)
              BoxShadow(
                color: (status.color ?? Theme.of(context).colorScheme.primary)
                    .withOpacity(.24),
                blurRadius: 14,
                spreadRadius: 2,
              ),
          ],
        ),
        child: ClipOval(
          child: imageUrl.isEmpty
              ? const Icon(Icons.movie_outlined, size: 48)
              : Image.network(
                  imageUrl,
                  fit: BoxFit.cover,
                  errorBuilder: (_, __, ___) => const Icon(Icons.broken_image),
                ),
        ),
      ),
    );
  }

  Widget _titleCard(dal.GraphNode node, {required bool expanded}) {
    final title = SizedBox(
      width: 148,
      height: expanded ? 58 : 48,
      child: Center(
        child: AutoSizeText(
          node.title ?? '',
          maxLines: 3,
          minFontSize: 9,
          textAlign: TextAlign.center,
        ),
      ),
    );

    if (!expanded) {
      return Card(
        margin: EdgeInsets.zero,
        child: Padding(padding: const EdgeInsets.all(4), child: title),
      );
    }

    return Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(6),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              children: [
                Expanded(child: title),
                IconButton.filledTonal(
                  visualDensity: VisualDensity.compact,
                  onPressed: () => _onNodeTap(node),
                  icon: const Icon(Icons.open_in_new, size: 18),
                ),
              ],
            ),
            const SizedBox(height: 3),
            Wrap(
              alignment: WrapAlignment.center,
              spacing: 5,
              runSpacing: 4,
              children: [
                _badge(node.mean?.toString() ?? '?'),
                _badge(
                  '${node.startSeason?.season?.name.titleCase() ?? '?'} ${node.startSeason?.year ?? '?'}',
                ),
                _badge(node.mediaType?.standardize() ?? '?'),
                _badge(node.status?.standardize() ?? '?'),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _badge(String text) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(20),
      ),
      child: Text(text, style: const TextStyle(fontSize: 10)),
    );
  }

  void _toggleExpanded(dal.GraphNode node) {
    if (node.id == null) return;
    setState(() {
      if (!_expandedIds.add(node.id!)) {
        _expandedIds.remove(node.id!);
      }
      _selectedId = node.id!;
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _graphController.forceRecalculation();
      _graphController.animateToNode(ValueKey(node.id));
    });
  }

  void _selectNode(dal.GraphNode node) {
    if (node.id == null) return;
    _selectedId = node.id!;
    setState(() {});
    _graphController.animateToNode(ValueKey(node.id));
  }

  void _onNodeTap(dal.GraphNode node) {
    gotoPage(
      context: context,
      newPage: ContentDetailedScreen(
        node: dal.Node(
          id: node.id,
          title: node.title,
          mainPicture: dal.Picture(
            large: node.mainPicture?.large,
            medium: node.mainPicture?.medium,
          ),
        ),
      ),
    );
  }

  void _showEdgeInfo() {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: Row(
          children: [
            Expanded(child: Text(S.current.Graph_Edge_Info)),
            IconButton(
              onPressed: () => Navigator.pop(context),
              icon: const Icon(Icons.close),
            ),
          ],
        ),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            _legend(S.current.Sequel, _sequelColor),
            _legend(S.current.Prequel, _prequelColor),
            _legend(S.current.Others, _otherColor),
            const SizedBox(height: 8),
            const Text(
              'Arrowheads point toward the destination anime.',
              style: TextStyle(fontSize: 12),
            ),
          ],
        ),
      ),
    );
  }

  Widget _legend(String label, Color color) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 7),
      child: Row(
        children: [
          Container(
            width: 42,
            height: 4,
            decoration: BoxDecoration(
              color: color,
              borderRadius: BorderRadius.circular(4),
            ),
          ),
          const SizedBox(width: 12),
          Text(label),
        ],
      ),
    );
  }

  Future<void> _captureAndSharePng() async {
    try {
      final boundary = _graphKey.currentContext?.findRenderObject()
          as RenderRepaintBoundary?;
      if (boundary == null) return;

      ui.Image? graphImage;
      for (final ratio in [4.0, 3.0, 2.0, 1.0]) {
        try {
          graphImage = await boundary.toImage(pixelRatio: ratio);
          break;
        } catch (_) {}
      }
      if (graphImage == null) {
        if (mounted) showToast(S.current.Couldnt_generate_graph);
        return;
      }

      final bytes = await graphImage.toByteData(format: ui.ImageByteFormat.png);
      if (bytes == null) return;

      final tempDir = await getTemporaryDirectory();
      final title = (_nodeMap[widget.id]?.title ?? 'anime_graph')
          .replaceAll(RegExp(r'[^\\w\\s-]'), '')
          .trim();
      final fileName = '${title.isEmpty ? 'anime_graph' : title}_${widget.id}_${DateTime.now().millisecondsSinceEpoch}.png';
      final file = File('${tempDir.path}/$fileName');
      await file.writeAsBytes(bytes.buffer.asUint8List(), flush: true);
      await Share.shareXFiles([XFile(file.path)], text: fileName);
    } catch (e) {
      dal.logDal(e.toString());
      if (mounted) showToast(S.current.Couldnt_generate_graph);
    }
  }
}
