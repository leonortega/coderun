# Contributing to Coderun

Thank you for your interest in contributing to Coderun! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- **Rust 1.75+** — Install via [rustup](https://rustup.rs/)
- **Git** — Version control
- **Node.js 20+** — For evaluation framework (optional)

### Getting Started

```bash
# Clone the repository
git clone https://github.com/your-org/coderun.git
cd coderun

# Build the project
cargo build

# Run tests
cargo test

# Run clippy for linting
cargo clippy

# Format code
cargo fmt
```

## Project Structure

```
coderun/
├── crates/
│   ├── coderun-core/        # Shared types, errors, config
│   ├── coderun-daemon/      # Daemon binary
│   ├── coderun-cli/         # CLI binary
│   ├── coderun-repo-intel/  # Repository Intelligence
│   ├── coderun-knowledge/   # Knowledge Hub
│   ├── coderun-skills/      # Skill Engine
│   ├── coderun-context/     # Context Engine
│   ├── coderun-router/      # Model Router
│   ├── coderun-optimizer/   # Execution Optimizer
│   ├── coderun-events/      # Event Bus
│   └── coderun-storage/     # Local Storage
├── eval/                    # Evaluation framework
├── docs/                    # Documentation
└── .coderun/                # Default configuration
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
```

### 2. Make Changes

- Follow existing code style
- Add tests for new functionality
- Update documentation if needed

### 3. Run Checks

```bash
# Build
cargo build

# Test
cargo test

# Lint
cargo clippy

# Format
cargo fmt --check
```

### 4. Commit

```bash
git add .
git commit -m "feat: add your feature description"
```

Use [Conventional Commits](https://www.conventionalcommits.org/) format:
- `feat:` — New feature
- `fix:` — Bug fix
- `docs:` — Documentation changes
- `style:` — Code style changes (formatting, etc.)
- `refactor:` — Code refactoring
- `test:` — Adding or updating tests
- `chore:` — Maintenance tasks

### 5. Push and Create PR

```bash
git push origin feature/your-feature-name
```

Create a Pull Request with:
- Clear description of changes
- Reference to any related issues
- Screenshots if applicable

## Code Style

### Rust

- Follow the [Rust Style Guide](https://github.com/rust-lang/style-team)
- Use `cargo fmt` to format code
- Use `cargo clippy` to catch common mistakes
- Prefer explicit types over complex type inference
- Add documentation comments for public items

### Testing

- Write unit tests for all new functionality
- Use descriptive test names
- Test both success and error cases
- Use `#[cfg(test)]` module for tests

Example:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        // Arrange
        let input = "test";

        // Act
        let result = my_function(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

### Documentation

- Add doc comments for all public items
- Include examples in documentation
- Update README for user-facing changes
- Update relevant docs in `docs/` directory

## Adding a New Crate

1. Create `crates/coderun-<name>/Cargo.toml`
2. Add to workspace `Cargo.toml` members
3. Add shared dependencies to `[workspace.dependencies]`
4. Create `src/lib.rs` with module code
5. Add tests in `#[cfg(test)] mod tests`
6. Update README.md project structure

## Adding a New Feature

1. Create or update the relevant crate
2. Add unit tests
3. Add integration tests if applicable
4. Update documentation
5. Add to evaluation framework if applicable

## Reporting Issues

When reporting issues, please include:

1. **Description** — What happened vs. what you expected
2. **Steps to reproduce** — How to trigger the issue
3. **Environment** — OS, Rust version, etc.
4. **Logs** — Relevant error messages or logs

## Code of Conduct

- Be respectful and inclusive
- Focus on constructive feedback
- Help newcomers learn
- Celebrate successes

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
