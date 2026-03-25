# pecto

**Extract behavior specs from code through static analysis.**

> Your code has behaviors. pecto makes them visible.

pecto is a Rust-based CLI tool that analyzes your codebase and generates human-readable behavior specifications — without LLMs, fast, deterministic, and fully offline.

## Why

- 76% of developers using AI tools generate code they don't fully understand
- Legacy codebases grow faster than documentation
- Existing tools find *bugs* — pecto extracts *behavior*
- Code review shows *what changed* — pecto shows *what the code does*

## Quick Start

```bash
# Install
cargo install pecto

# Analyze a Java/Spring Boot project
pecto init ./my-spring-app

# Analyze a C#/.NET project (auto-detected)
pecto init ./my-dotnet-app

# Explicitly specify language
pecto init ./my-app --language csharp

# Show a specific capability
pecto show user-authentication

# Check for behavior drift
pecto verify spec.yaml

# Compare behaviors between git refs
pecto diff main HEAD
```

## Example Output

```yaml
capability: user-authentication
  source: src/main/java/com/app/auth/AuthController.java

  endpoint: POST /api/auth/login
    input:
      body: LoginRequest {email: String, password: String}
    validation:
      - email: @NotBlank, @Email
      - password: @NotBlank, @Size(min=8)
    behavior:
      success:
        returns: 200 AuthResponse {token: String, refreshToken: String}
      invalid_credentials:
        returns: 401 ErrorResponse {message: "Invalid credentials"}
        side_effects:
          - update: user.failedAttempts += 1
          - when: failedAttempts >= 5
            action: lock account for 30min
    security:
      authentication: none (public endpoint)
      rate_limit: 20/min per IP
```

## Language Support

| Language | Frameworks | Status |
|----------|-----------|--------|
| **Java** | Spring Boot, JPA/Hibernate, Spring Security | Ready |
| **C#** | ASP.NET Core, Entity Framework, DI | Ready |
| **Python** | Django, Flask, FastAPI | Planned |
| **TypeScript** | Express, NestJS, Next.js | Planned |

## How It Works

pecto uses [tree-sitter](https://tree-sitter.github.io/) for fast, accurate parsing and extracts behavior through static analysis:

1. **Parse** — Build AST for every source file
2. **Detect** — Identify frameworks, patterns, and annotations
3. **Extract** — Pull out endpoints, operations, entities, security rules
4. **Generate** — Output structured behavior specs (YAML/JSON)
5. **Verify** — Check that code still matches its spec

No LLM required. No API keys. No cloud. Just fast, deterministic analysis.

## License

MIT
