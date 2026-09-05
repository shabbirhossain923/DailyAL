import 'sync_account.dart';

class SyncChange {
  final SyncProvider provider;
  final String animeId;
  final String field;
  final Object? localValue;
  final Object? remoteValue;

  const SyncChange({
    required this.provider,
    required this.animeId,
    required this.field,
    required this.localValue,
    required this.remoteValue,
  });
}

class SyncResult {
  final SyncProvider provider;
  final int imported;
  final int exported;
  final int conflicts;
  final List<String> errors;

  const SyncResult({
    required this.provider,
    this.imported = 0,
    this.exported = 0,
    this.conflicts = 0,
    this.errors = const [],
  });

  bool get hasErrors => errors.isNotEmpty;
}
