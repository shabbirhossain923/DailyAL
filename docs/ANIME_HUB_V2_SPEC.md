# Anime Hub v2 — Product & Architecture Specification

## Goal

Evolve DailyAL into a cross-platform anime tracking application that keeps the strengths of DailyAL while adding AniList + MyAnimeList dual synchronization, community reviews, richer statistics/graphs, and optional AI-assisted discovery.

## Non-goals for the first milestone

- Do not replace the existing DailyAL production branch.
- Do not build a custom anime database before validating the external data sources.
- Do not make AI a dependency for core tracking or synchronization.

## Core features

### 1. Unified anime library

- Watching
- Completed
- On Hold
- Dropped
- Plan to Watch
- Favorites
- Episode progress
- Personal score
- Start/finish dates
- Notes

The app maintains one canonical internal anime record and maps it to external IDs from MAL and AniList.

### 2. MAL + AniList dual sync

Users can connect either or both services.

Required behavior:

- Import library from MAL
- Import library from AniList
- Link matching anime records using external IDs and verified title/metadata matching
- Push local changes to selected services
- Pull remote changes
- Manual full sync
- Incremental sync where possible
- Per-service connection status and last-sync timestamp
- Conflict resolution UI
- User-configurable sync authority: local, MAL, AniList, or ask on conflict

Important: synchronization must be explicit and auditable. Never silently overwrite a user's list because two services disagree.

### 3. Community reviews

Each anime can have reviews from users of this application.

Review model:

- Rating
- Review body
- Spoiler flag
- Created/updated timestamp
- Helpful vote
- Reply thread
- Reviewer profile
- Optional structured tags such as story, characters, animation, pacing, and ending

Sorting:

- Most helpful
- Newest
- Highest rated

Moderation requirements:

- Report review
- Delete/edit own review
- Basic anti-spam/rate limiting
- Spoiler-safe display

### 4. Statistics and GraphView

Preserve the useful DailyAL graph experience while redesigning the data layer so graphs are generated from normalized local history.

Initial graphs:

- Episode progress over time
- Completed anime over time
- Watching activity by week/month
- Personal rating distribution
- Genre distribution
- Watch-time estimate
- Rating history
- MAL vs AniList library counts

The graph system must work even when the user is offline after data has been cached locally.

### 5. Anime detail page

Sections:

- Poster/banner
- Title variants
- Synopsis
- Airing/release information
- Episodes/seasons
- Genres/themes
- Studio/staff/characters where available
- User status/progress
- Personal rating
- Community rating
- Community reviews
- Recommendations/similar anime
- Statistics

### 6. Home dashboard

- Continue Watching
- Recently Updated
- Upcoming Episodes
- Airing Now
- Personal Recommendations
- Quick progress controls
- Sync status

### 7. Optional AI layer

AI is a secondary layer and must not block basic app functionality.

Potential features:

- Explain why an anime matches the user's taste
- Natural-language anime search
- Summarize community sentiment from reviews
- Detect common praise/criticism themes
- Personalized recommendations

AI summaries must be clearly labeled as AI-generated and must not be presented as direct human opinions.

## Architecture direction

The current DailyAL branch is Flutter-based and already contains MAL-oriented app infrastructure, local preferences/secure storage, notifications, caching, and graph-related dependencies. The new work should reuse stable components where practical instead of rewriting everything at once.

Recommended layers:

```text
Presentation
  -> Feature modules / screens / widgets

Application
  -> Library service
  -> Sync coordinator
  -> Review service
  -> Statistics service

Domain
  -> Anime
  -> UserAnimeEntry
  -> Review
  -> ExternalAnimeMapping
  -> SyncConflict
  -> WatchEvent

Data
  -> Local database/cache
  -> MAL adapter
  -> AniList GraphQL adapter
  -> Community API adapter
  -> AI adapter
```

## Canonical data model principles

The local domain model is authoritative for UI rendering. External services are integrations, not the domain model itself.

Every tracked anime should support:

```text
animeId
malId?
anilistId?
title variants
media metadata
status
progress
score
startDate?
finishDate?
lastUpdated
```

Every synchronization operation should record:

```text
provider
operation
startedAt
finishedAt
result
conflicts
error?
```

## Sync strategy

Use a provider adapter interface so MAL and AniList implementations are independent.

```text
SyncCoordinator
  ├── MalSyncProvider
  └── AniListSyncProvider
```

The coordinator compares normalized records instead of comparing raw API payloads.

Conflict examples:

- Local episode = 8, MAL = 7, AniList = 8 -> safe resolution can be 8.
- Local score differs from MAL and AniList -> ask user unless a sync policy is configured.
- Status differs between providers -> show a conflict rather than guessing.

## Security

- Store OAuth/access tokens only in secure storage.
- Never commit API secrets to Git.
- Keep provider credentials separate from user review data.
- Request the minimum practical permissions supported by each provider.
- Provide disconnect/revoke controls.

AniList currently uses OAuth2; authenticated operations are required for modifying user lists. Its official API is GraphQL-based. See the official documentation before implementing the adapter.

## Milestones

### M0 — Foundation

- Freeze `graph-final` as the stable baseline.
- Work on `anime-hub-v2`.
- Inventory existing MAL, graph, cache, authentication, and data-model code.
- Define normalized domain models.

### M1 — New shell/UI

- Design system
- Home
- Search
- Anime detail
- Library
- Statistics
- Profile/settings
- Sync settings

Use Google Stitch for rapid high-fidelity UI exploration; export the resulting frontend/design artifacts into the development workflow rather than treating Stitch as the backend.

### M2 — Unified library

- Local persistence
- Normalized anime model
- Import existing DailyAL state
- Library CRUD
- History/watch events

### M3 — Dual sync

- MAL adapter
- AniList adapter
- ID mapping
- Sync coordinator
- Conflict resolution
- Sync logs

### M4 — Community

- Authentication for the app itself
- Reviews
- Ratings
- Helpful votes
- Replies
- Moderation/reporting

### M5 — Graphs

- Rebuild DailyAL graph experience on normalized history
- Add new statistics
- Offline-friendly cached graph data

### M6 — AI

- Natural-language search
- Review sentiment/theme summary
- Personalized recommendation explanations

### M7 — Release

- Android build
- PWA/web build if supported by the chosen architecture
- Error reporting
- Performance testing
- Sync reliability testing
- Privacy/security review

## Immediate next action

Before changing production behavior, inspect and map the existing DailyAL implementation for:

1. MAL authentication and API calls
2. Existing anime/manga domain models
3. Existing user-list update logic
4. Existing GraphView implementation
5. Local cache/storage
6. Existing notification/airing logic
7. Existing review/recommendation screens

Then implement the normalized domain layer and provider interfaces on `anime-hub-v2`.
