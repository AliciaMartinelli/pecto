# pecto

[![CI](https://github.com/pecto-dev/pecto/actions/workflows/ci.yml/badge.svg)](https://github.com/pecto-dev/pecto/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Extract behavior specs from code through static analysis.**

> Your code has behaviors. pecto makes them visible.

pecto analyzes your Java and C# codebases and generates human-readable behavior specifications — without LLMs, fast, deterministic, and fully offline.

```
$ pecto init ./spring-petclinic-rest

pecto Analyzing /home/dev/spring-petclinic-rest...
✓ Analyzed 86 files → 18 capabilities

  8 entities
  7 repositories (45 operations)
  2 services (30 operations)
  1 controller (1 endpoint)
```

## Why

- 76% of developers using AI tools generate code they don't fully understand
- Legacy codebases grow faster than documentation
- Existing tools find *bugs* — pecto extracts *behavior*
- Code review shows *what changed* — pecto shows *what the code does*

## Install

```bash
# From source
cargo install pecto

# Or download binary from releases
# https://github.com/pecto-dev/pecto/releases
```

## Quick Start

```bash
# Analyze a project (language auto-detected)
pecto init ./my-app

# Specify language explicitly
pecto init ./my-app --language java
pecto init ./my-app --language csharp

# Output as JSON instead of YAML
pecto init ./my-app --format json

# Save to file
pecto init ./my-app --output specs.yaml

# Show detailed per-capability breakdown
pecto init ./my-app --verbose

# Machine-readable output (no status messages)
pecto init ./my-app --quiet > specs.yaml
```

## Commands

### `pecto init` — Analyze a project

```bash
pecto init [path] [--format yaml|json] [--output file] [--verbose] [--quiet]
```

Scans the project, extracts behavior specs, outputs to stdout or file.

### `pecto show` — Inspect a capability

```bash
pecto show <name> [--path .]
```

Shows the spec for a specific capability. Supports substring matching.

### `pecto verify` — Detect behavior drift

```bash
pecto verify <spec-file> [--path .]
```

Compares an existing spec against the current code. Exits with code 1 if drift is detected. Ideal for CI pipelines.

### `pecto diff` — Compare git refs

```bash
pecto diff <base-ref> [head-ref] [--path .]
```

Shows behavior changes between two git refs. Useful for code review.

## Example Output

```yaml
name: spring-petclinic-rest
analyzed: '2026-03-25T14:30:00Z'
files_analyzed: 86
capabilities:
- name: owner-entity
  source: model/Owner.java
  entities:
  - name: Owner
    table: owners
    fields:
    - name: id
      type: Integer
      constraints: ["@Id", "@GeneratedValue"]
    - name: firstName
      type: String
      constraints: ["@NotBlank"]
    - name: lastName
      type: String
      constraints: ["@NotBlank"]

- name: owner-repository
  source: repository/OwnerRepository.java
  operations:
  - name: save
    source_method: SpringDataOwnerRepository#save
  - name: findById
    source_method: SpringDataOwnerRepository#findById
  - name: Find by last name
    source_method: SpringDataOwnerRepository#findByLastName

- name: clinic-service
  source: service/ClinicService.java
  operations:
  - name: findOwnerById
    source_method: ClinicServiceImpl#findOwnerById
    transaction: read-only
```

## Language Support

### Java

| Feature | What pecto extracts |
|---------|-------------------|
| **Spring Controllers** | `@RestController`, `@GetMapping`/`@PostMapping`/etc., path variables, request params, request body |
| **JPA Entities** | `@Entity`, `@Table`, field types, `@Id`, `@Column`, relationships (`@OneToMany`, `@ManyToOne`) |
| **Spring Data Repos** | `JpaRepository<T, ID>`, CRUD operations, custom query methods, `@Query` |
| **Services** | `@Service`, public methods, `@Transactional`, side effects (DB ops, events, service calls) |
| **Validation** | `@Valid` with cross-file DTO resolution: `@NotBlank`, `@Email`, `@Size`, `@Min`, `@Max`, `@Pattern` |
| **Security** | `@PreAuthorize`, `@Secured`, `@CrossOrigin`, `@RateLimiter` |
| **Error Handling** | `throw ResponseStatusException`, `ResponseEntity.notFound()`, `@ExceptionHandler` |
| **Scheduled Tasks** | `@Scheduled(cron=...)`, `@EventListener` |

### C# / .NET

| Feature | What pecto extracts |
|---------|-------------------|
| **ASP.NET Controllers** | `[ApiController]`, `[HttpGet]`/`[HttpPost]`/etc., `[Route]` with `[controller]` substitution |
| **EF Core Entities** | `DbSet<T>` in DbContext, `[Table]`, `[Key]`, `[Required]`, `[MaxLength]`, `[ForeignKey]` |
| **Services** | Naming convention (*Service) or interface (I*Service), public methods, transaction patterns |
| **Validation** | `[FromBody]` with cross-file resolution: `[Required]`, `[EmailAddress]`, `[Range]`, `[StringLength]` |
| **Security** | `[Authorize]`, `[Authorize(Roles = "...")]`, `[AllowAnonymous]` |
| **Error Handling** | `[ProducesResponseType(StatusCodes.Status404NotFound)]` |
| **Background Tasks** | `BackgroundService`, `IHostedService`, `TimeSpan.From*` timer patterns |
| **Parameters** | `[FromBody]`, `[FromRoute]`, `[FromQuery]`, async `Task<ActionResult<T>>` unwrapping |

## Performance

| Project | Files | Capabilities | Time |
|---------|-------|-------------|------|
| Spring PetClinic REST (Java) | 86 | 18 | ~0.5s |
| eShopOnWeb (C#/.NET) | 254 | 18 | ~1.6s |

Parallel file parsing with [rayon](https://github.com/rayon-rs/rayon). No network calls. No LLM. Pure tree-sitter static analysis.

## How It Works

pecto uses [tree-sitter](https://tree-sitter.github.io/) for fast, accurate parsing:

1. **Parse** — Build AST for every source file (parallel with rayon)
2. **Detect** — Identify frameworks, patterns, and annotations/attributes
3. **Extract** — Pull out endpoints, operations, entities, security rules, side effects
4. **Resolve** — Cross-file type resolution (DTOs, entities, DbContext)
5. **Generate** — Output structured behavior specs (YAML/JSON)

## CI Integration

Use `pecto verify` in your CI pipeline to detect behavior drift:

```yaml
# .github/workflows/pecto.yml
- name: Check behavior spec
  run: |
    cargo install pecto
    pecto verify specs.yaml --path .
```

Exit code 1 means the spec has drifted from the code.

## Contributing

```bash
# Clone and build
git clone https://github.com/pecto-dev/pecto
cd pecto
cargo build

# Run tests
cargo test

# Run integration tests (requires internet for git clone)
cargo test -- --ignored

# Check formatting and lints
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## License

MIT
