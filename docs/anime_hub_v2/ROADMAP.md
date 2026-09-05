# Anime Hub v2 Roadmap

Anime Hub v2 is the next-generation anime tracker built from the DailyAL Flutter foundation.

## Product goals
- One unified anime library backed by MyAnimeList (MAL) and AniList.
- Reliable dual-sync with explicit conflict handling.
- Community reviews with spoiler controls and helpful votes.
- DailyAL-style statistics/graphs, expanded with history and comparisons.
- Optional AI summaries and personalized recommendations.
- Android-first UX with offline-friendly caching.

## Phases

### Phase 0 — Foundation
- [x] Create `anime-hub-v2` development branch.
- [x] Define unified domain model.
- [ ] Audit existing MAL/auth/list/cache/graph code for reuse.
- [ ] Add provider interfaces and sync orchestration.
- [ ] Add local persistence and migration strategy.

### Phase 1 — Library + sync
- [ ] MAL authentication and read/write sync.
- [ ] AniList authentication and read/write sync.
- [ ] Stable MAL↔AniList media identity mapping.
- [ ] Bidirectional sync with per-field conflict policy.
- [ ] Manual sync, automatic sync, retry and error reporting.

### Phase 2 — UI
- [ ] Home dashboard.
- [ ] Search/explore.
- [ ] Anime details.
- [ ] Library/status views.
- [ ] Profile and sync settings.
- [ ] Statistics dashboard.

### Phase 3 — Community
- [ ] User reviews.
- [ ] Ratings.
- [ ] Spoiler-safe review display.
- [ ] Helpful voting.
- [ ] Moderation/reporting primitives.

### Phase 4 — Analytics
- [ ] Watch-history event model.
- [ ] Rating/progress graphs.
- [ ] Genre and score distributions.
- [ ] MAL vs AniList comparison.
- [ ] Weekly/monthly/yearly activity.

### Phase 5 — AI
- [ ] Review summarization with source attribution.
- [ ] Personalized recommendation explanations.
- [ ] Preference analysis.
- [ ] AI features remain optional and clearly separated from community opinions.

## Non-negotiable design rules
1. Never silently overwrite one service with another.
2. Every sync mutation must be retryable and idempotent where possible.
3. Community review text is user-generated content; AI summaries must never be presented as human reviews.
4. Store immutable watch-history events so graphs can be rebuilt accurately.
5. Keep provider-specific IDs; never use MAL or AniList IDs as the internal primary key.
