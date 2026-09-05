# Anime Hub v2 — implementation plan

## Product goal

Build a Flutter anime tracker that extends the existing DailyAL experience with a unified local model, MAL + AniList dual sync, richer watch-history graphs, and first-class community reviews.

## Architecture

```text
UI
  -> application services
      -> unified domain models
          -> provider adapters
              -> MyAnimeList / AniList
```

The UI must not depend directly on MAL or AniList response objects.

## Milestones

### 1. Foundation
- Unified anime identity: internal ID + MAL ID + AniList ID.
- User anime entry with status, progress, score and timestamps.
- Append-only watch-history events for graph/statistics reconstruction.
- Provider-neutral sync account and conflict models.
- Community review model with spoiler flag and helpful votes.

### 2. MAL adapter
- Reuse the existing MAL authentication and API implementation where possible.
- Map MAL list entries into the unified user-entry model.
- Preserve remote IDs and provider metadata.

### 3. AniList adapter
- Add AniList GraphQL client and OAuth flow.
- Read the authenticated viewer and anime list.
- Map AniList entries into the same unified model.
- Keep AniList credentials in secure storage.

### 4. Dual-sync engine
- Pull both providers into local state.
- Match records by known cross-provider IDs first, then conservative metadata matching.
- Never silently overwrite a conflicting progress/score/status value.
- Record conflicts and expose a user-selectable resolution policy.
- Push local changes to connected providers only after a deterministic merge.

### 5. Graphs and statistics
- Reuse the existing graph dependencies where appropriate.
- Build graphs from watch-history events rather than current list state alone.
- Add daily/weekly/monthly activity, score distribution, genre distribution and provider comparison.

### 6. Community reviews
- Review creation/edit/delete.
- Spoiler protection.
- Helpful voting.
- Sorting by helpfulness, recency and rating.
- Report/moderation hooks before public launch.

### 7. UI
- Home dashboard.
- Search/discovery.
- Anime details.
- Library.
- Reviews.
- Statistics.
- Profile.
- Sync settings.

### 8. AI
- Review summarization with clear AI labeling.
- Personal recommendation based on the user's own history.
- Explainable recommendation reasons.

## Important API constraints

AniList uses GraphQL and authenticated requests are required for modifying user lists. Its OAuth implementation supports an implicit flow for client-side apps and an authorization-code flow where credentials can be kept securely. Tokens expire and AniList currently does not provide refresh tokens, so re-authentication handling is required.

MAL credentials must also remain outside source control. Existing DailyAL credential/configuration patterns should be audited before any new sync code is added.

## Definition of done for the first usable build

A user can connect MAL, optionally connect AniList, import both lists into one library, update an anime's progress locally, inspect watch-history graphs, and see which provider changes are pending before they are written remotely.
