/// Provider-neutral anime identity and metadata.
///
/// Provider IDs are intentionally kept alongside an internal ID so MAL and
/// AniList can be synchronized without making either provider authoritative.
class AnimeMedia {
  const AnimeMedia({
    required this.id,
    this.malId,
    this.anilistId,
    required this.title,
    this.episodes,
    this.status,
    this.coverImageUrl,
  });

  final String id;
  final int? malId;
  final int? anilistId;
  final String title;
  final int? episodes;
  final String? status;
  final String? coverImageUrl;
}
