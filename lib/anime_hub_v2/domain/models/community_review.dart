class CommunityReview {
  final String id;
  final String animeId;
  final String authorId;
  final String authorName;
  final double rating;
  final String text;
  final bool containsSpoiler;
  final int helpfulVotes;
  final DateTime createdAt;
  final DateTime? updatedAt;

  const CommunityReview({
    required this.id,
    required this.animeId,
    required this.authorId,
    required this.authorName,
    required this.rating,
    required this.text,
    this.containsSpoiler = false,
    this.helpfulVotes = 0,
    required this.createdAt,
    this.updatedAt,
  });
}
