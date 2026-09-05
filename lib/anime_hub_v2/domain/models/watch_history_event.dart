enum WatchHistoryAction {
  progressChanged,
  statusChanged,
  scoreChanged,
}

/// Immutable event used to rebuild statistics and graphs.
class WatchHistoryEvent {
  const WatchHistoryEvent({
    required this.id,
    required this.animeId,
    required this.action,
    required this.timestamp,
    this.fromValue,
    this.toValue,
  });

  final String id;
  final String animeId;
  final WatchHistoryAction action;
  final DateTime timestamp;
  final String? fromValue;
  final String? toValue;
}
