enum AnimeListStatus {
  watching,
  completed,
  onHold,
  dropped,
  planToWatch,
}

/// Provider-neutral user state for an anime.
class UserAnimeEntry {
  const UserAnimeEntry({
    required this.animeId,
    required this.status,
    this.progress = 0,
    this.score,
    this.updatedAt,
  });

  final String animeId;
  final AnimeListStatus status;
  final int progress;
  final double? score;
  final DateTime? updatedAt;
}
