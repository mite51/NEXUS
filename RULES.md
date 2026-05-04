# NEXUS Project Rules

## Development Disciplines

### Code Quality
- **Clean code**: Readable, well-named, minimal complexity. If a human can't understand it quickly, refactor.
- **No duplication**: DRY principle enforced. Extract shared logic into modules.
- **Minimize external dependencies**: Prefer standard library solutions. Every dependency must justify its existence.
- **Cross-platform by design**: All code must work on Linux, macOS, and Windows. Abstract platform-specific logic behind interfaces.
- **Beautiful simplicity**: If an implementation is getting too complicated, stop and refactor. Complexity is a bug.

### Testing
- **Unit tests required** for all major features.
- **Mocks only where necessary** — prefer real implementations in tests when feasible.
- **All tests must pass before any commit.** No exceptions.

### Version Control
- **Changelog**: Maintain `CHANGELOG.md` with entries for all major features, breaking changes, and architectural decisions.
- **No broken commits**: Every commit should leave the project in a buildable, test-passing state.

### Architecture
- **Present options to the user** when a major architectural pattern could either cause downstream problems OR solve multiple problems at once.
- **Keep the SRD updated** when architectural decisions change the technical design.
- **Refactor early**: If complexity is creeping in, propose a refactor before it compounds.

### Agent Workflow
- **Spawn sub-agents** with appropriate context for isolated feature work when reasonable.
- **Keep context tight**: Each agent should have clear scope and deliverables.

---

*These rules are enforced for all contributions to the NEXUS project.*
