# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Phase 0: Project scaffolding with 11-crate workspace
- Phase 1: Configuration system with TOML loading and env overrides
- Phase 2: Core types, error enums, and IPC message types
- Phase 3: Event Bus with broadcast channel and in-memory buffer
- Phase 4: Local Storage with SQLite, WAL mode, and migrations
- Phase 5: Repository Intelligence with incremental indexing and search
- Phase 6: Skill Engine with Markdown/TOML/YAML parsing
- Phase 7: Knowledge Hub with storage, search, and extraction
- Phase 8: Model Router with complexity scoring and tier selection
- Phase 9: Execution Optimizer with type-specific compression
- Phase 10: Context Engine with pipeline assembly and token budget
- Phase 11: Adapter Layer with UDS/TCP and MessagePack IPC
- Phase 12: Daemon Lifecycle with startup, shutdown, and signal handling
- Phase 13: CLI Commands (init, index, preview, status, skills, config, doctor)
- Phase 14: Agent Adapters for OpenCode and Claude Code
- Phase 15: Evaluation Framework with Promptfoo
- Phase 16: Hardening and documentation

### Changed
- Nothing yet

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Fixed
- Nothing yet

### Security
- Passed cargo audit with zero vulnerabilities

## [0.1.0] - 2026-08-24

### Added
- Initial release with all core components
- 106 unit tests passing
- Zero clippy warnings
- Zero security vulnerabilities

[Unreleased]: https://github.com/your-org/coderun/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/your-org/coderun/releases/tag/v0.1.0
