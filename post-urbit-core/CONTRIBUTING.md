# Contributing to Post-Urbit

Thank you for your interest in contributing to Post-Urbit! This document provides guidelines and information for contributors.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Submitting Pull Requests](#submitting-pull-requests)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Documentation](#documentation)

## Code of Conduct

Be respectful and constructive in all interactions. We welcome contributors of all experience levels and backgrounds.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally
3. Set up the development environment (see below)
4. Create a branch for your changes
5. Make your changes and test them
6. Submit a pull request

## Development Setup

### Prerequisites

- **Rust**: 1.70 or later (`rustup` recommended)
- **Node.js**: 18 or later
- **npm**: 8 or later

### Backend Setup

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/post-urbit
cd post-urbit/post-urbit-core

# Build the project
cargo build

# Run tests
cargo test

# Run the node in development mode
cargo run -- run --dev --verbose
```

### Frontend Setup

```bash
# Navigate to the shell package
cd packages/shell

# Install dependencies
npm install

# Start the development server
npm run dev

# Build for production
npm run build
```

### Running Both Together

1. Start the backend: `cargo run -- run --dev`
2. In another terminal, start the frontend: `cd packages/shell && npm run dev`
3. Open `http://localhost:5173` in your browser

## Making Changes

### Branch Naming

Use descriptive branch names:

- `feature/add-contact-search` - New features
- `fix/message-delivery-timeout` - Bug fixes
- `docs/update-api-reference` - Documentation changes
- `refactor/simplify-auth-flow` - Code refactoring

### Commit Messages

Write clear, concise commit messages:

```
Add contact search functionality

- Implement fuzzy search in contacts list
- Add search input component with debouncing
- Update API to support search query parameter
```

Follow these guidelines:
- Use the imperative mood ("Add feature" not "Added feature")
- First line should be 50 characters or less
- Add a blank line before detailed description if needed
- Reference issue numbers where applicable (`Fixes #123`)

## Submitting Pull Requests

### Before Submitting

1. **Run tests**: Ensure all tests pass
   ```bash
   cargo test
   ```

2. **Check formatting**: Run the formatter
   ```bash
   cargo fmt
   ```

3. **Run linter**: Check for common issues
   ```bash
   cargo clippy
   ```

4. **Update documentation**: If your changes affect the API or user-facing features

### PR Description

Include in your pull request:

- **Summary**: What does this PR do?
- **Motivation**: Why is this change needed?
- **Testing**: How was this tested?
- **Screenshots**: For UI changes, include before/after screenshots

### Review Process

1. A maintainer will review your PR
2. Address any feedback or requested changes
3. Once approved, your PR will be merged

## Coding Standards

### Rust

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` for consistent formatting
- Address all `cargo clippy` warnings
- Write documentation comments for public APIs
- Use meaningful variable and function names

```rust
// Good
pub fn send_message(recipient: &IID, content: &[u8]) -> Result<MessageId, SendError> {
    // ...
}

// Avoid
pub fn send(r: &IID, c: &[u8]) -> Result<MessageId, SendError> {
    // ...
}
```

### TypeScript/React

- Use TypeScript for all new code
- Follow the existing component patterns
- Keep components focused and composable
- Use React hooks appropriately

```typescript
// Good
const ContactList: React.FC<ContactListProps> = ({ contacts, onSelect }) => {
  // ...
};

// Avoid inline styles for complex styling
// Use the System 7 component library
```

### System 7 UI Guidelines

When contributing to the frontend, follow the [Visual Design Spec](docs/specs/10-VISUAL_DESIGN.md):

- Use the established color palette (grayscale chrome, System 7 accents)
- Maintain pixel-perfect aesthetics where possible
- Use Chicago-style typography
- Follow beveled 3D control patterns

## Testing

### Backend Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_message_encryption

# Run tests with output
cargo test -- --nocapture
```

### Writing Tests

- Write unit tests for new functions
- Add integration tests for API endpoints
- Test error cases, not just happy paths

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iid_derivation() {
        let keypair = generate_keypair();
        let iid = derive_iid(&keypair.public);
        assert!(iid.len() == 32);
    }

    #[test]
    fn test_invalid_input_returns_error() {
        let result = process_input("");
        assert!(result.is_err());
    }
}
```

## Documentation

### When to Update Docs

- Adding new API endpoints
- Changing existing behavior
- Adding new features
- Fixing bugs that affect documented behavior

### Documentation Locations

| Content | Location |
|---------|----------|
| API reference | `docs/api/http-api.md` |
| Architecture | `docs/README.md` |
| App development | `docs/apps/building-apps.md` |
| Technical specs | `docs/specs/` |
| Frontend | `packages/shell/README.md` |

### Writing Style

- Use clear, concise language
- Include code examples where helpful
- Keep formatting consistent with existing docs
- Use tables for structured information

## Questions?

If you have questions about contributing:

1. Check existing documentation
2. Search closed issues for similar questions
3. Open a new issue with the "question" label

Thank you for contributing to Post-Urbit!
