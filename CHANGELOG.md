# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-03-08

### Added
- Initial release of Skills Manager v2 with Tauri backend
- Scenario management: create, rename, delete, and switch scenarios
- Scenario icons and sync engine improvements
- Light/dark theme support with system preference detection
- Global search dialog and help dialog
- Configurable sync mode and startup scenario sync
- External link button for market skill cards
- Market search/filter, error banners, and enhanced confirm dialog
- Skill update checking and updating for git-based skills
- Load-more pagination for market skill list
- Skill deduplication: check central path before installing

### Changed
- Redesigned MySkills card and list layout for compactness
- Unified UI styling with compact, consistent design system
- Paginate market skill list and flatten local scan UI
- Consolidated skill card metadata into a single priority-based status badge
- Compact skill card and list row layout with inline action buttons
- Compact market toolbar layout and redesigned skill cards
- Simplified local install section UI
- Improved skill detail panel rendering and market card layout
- Introduced shared app-page utility classes and standardized UI layout
- Removed global search and topbar; added help button to settings
- Updated app icons

### Fixed
- Replaced CSS `-webkit-app-region` drag with programmatic Tauri drag bar
- Replaced Hammer icon with custom app logo image in sidebar
