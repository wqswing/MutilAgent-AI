# Contributing to Multiagent

Thank you for your interest in contributing to Multiagent! We welcome contributions from the community.

## How to Contribute

### Reporting Issues

- Search existing issues before creating a new one
- Use clear, descriptive titles
- Include steps to reproduce bugs
- Specify your environment (OS, Rust version)

### Pull Requests

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes with clear commit messages
4. Ensure all tests pass: `cargo test --workspace`
5. Run clippy: `cargo clippy --workspace`
6. Submit a pull request

### Code Style

- Follow Rust standard formatting: `cargo fmt`
- Add documentation for public APIs
- Include tests for new features
- Keep commits atomic and focused

### Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for system design details.

## Development Setup

```bash
# Clone the repository
git clone https://github.com/wqswing/MultiAgent-AI.git
cd MultiAgent-AI

# Build
cargo build

# Run tests
cargo test --workspace

# Run with Docker
docker-compose up -d
cargo run
```

## License

By contributing, you agree that your contributions will be licensed under the AGPLv3 License.
