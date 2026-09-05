enum SyncProvider { mal, anilist }

enum SyncState { disconnected, connected, syncing, error }

class SyncAccount {
  final SyncProvider provider;
  final String externalUserId;
  final String username;
  final SyncState state;
  final DateTime? lastSyncedAt;
  final String? errorMessage;

  const SyncAccount({
    required this.provider,
    required this.externalUserId,
    required this.username,
    this.state = SyncState.connected,
    this.lastSyncedAt,
    this.errorMessage,
  });

  SyncAccount copyWith({
    SyncState? state,
    DateTime? lastSyncedAt,
    String? errorMessage,
  }) {
    return SyncAccount(
      provider: provider,
      externalUserId: externalUserId,
      username: username,
      state: state ?? this.state,
      lastSyncedAt: lastSyncedAt ?? this.lastSyncedAt,
      errorMessage: errorMessage ?? this.errorMessage,
    );
  }
}
