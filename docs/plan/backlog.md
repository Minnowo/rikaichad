# Backlog

Work the user parked to come back to. Not scheduled in a milestone.

- Group and merge lookup speed on a large collection. On the user's 99
  dictionaries, group is 9.0 ms p50 / 33.2 ms p99 and merge 12.1 / 76.8 ms
  (split 3.8 / 17.7 ms meets the targets). They return about ten times
  more content than split. Measurements: docs/plan/milestone-3.md.
- Import into a large existing store is slow (99 dictionaries: 879 s).
  Listed as open in docs/brief.md (Storage).
- CI on Linux, macOS and Windows (milestones 1 and 5 done-criteria): needs
  a GitHub repository.
- `rchad reimport`: re-import all dictionaries after a store format change.
