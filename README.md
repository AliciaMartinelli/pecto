# pecto

[![CI](https://github.com/AliciaMartinelli/pecto/actions/workflows/ci.yml/badge.svg)](https://github.com/AliciaMartinelli/pecto/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/pecto.svg)](https://crates.io/crates/pecto)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Extract behavior specs from code through static analysis.**

> Your code has behaviors. pecto makes them visible.

**[pecto.dev](https://pecto.dev)** · [Docs](https://pecto.dev/docs) · [Install](#install)

pecto is an open-source CLI that analyzes Java, C#, Python, and TypeScript codebases and generates human-readable behavior specifications — without LLMs, fast, deterministic, and fully offline.

```
$ pecto init ./my-spring-app

pecto Analyzing ./my-spring-app...
✓ Analyzed 86 files → 29 capabilities

  23 endpoints
  8 entities
  109 operations
```

## Why

- 76% of developers using AI tools generate code they don't fully understand
- Legacy codebases grow faster than documentation
- Existing tools find *bugs* — pecto extracts *behavior*
- Code review shows *what changed* — pecto shows *what the code does*

## Install

```bash
cargo install pecto
```

Or download pre-built binaries from [GitHub Releases](https://github.com/AliciaMartinelli/pecto/releases).

## Quick Start

```bash
# Analyze a project (language auto-detected)
cd ./my-app
pecto init

# Launch interactive dashboard
pecto serve

# Specify language explicitly
pecto init --language java

# Output as JSON instead of YAML
pecto init --format json

# Save to file
pecto init --output specs.yaml

# Show detailed per-capability breakdown
pecto init --verbose

# Machine-readable output (no status messages)
pecto init ./my-app --quiet > specs.yaml

# See business domains and dependencies
pecto domains ./my-app
pecto graph ./my-app --format dot | dot -Tpng > graph.png

# Impact analysis: what breaks if you change this?
pecto impact Owner

# Interactive web dashboard
pecto serve ./my-app

# AI-ready project context
pecto context ./my-app
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

### `pecto flow` — Request flow tracing

```bash
pecto flow "POST /api/users"            # Mermaid sequence diagram
pecto flow "GET /users" --format json   # JSON output
```

Trace the request flow for an endpoint as a Mermaid sequence diagram.

### `pecto check` — Architecture rules

```bash
pecto check                        # uses .pecto/rules.yaml or defaults
pecto check --rules my-rules.yaml  # custom rules file
```

Validate architecture fitness rules against your codebase.

Built-in rules: `no-circular-dependencies`, `controllers-no-direct-db-access`, `all-endpoints-need-authentication`, `no-entity-without-validation`, `max-service-dependencies`.

Configure in `.pecto/rules.yaml`:
```yaml
rules:
  no-circular-dependencies: true
  controllers-no-direct-db-access: true
  all-endpoints-need-authentication: true
  max-service-dependencies: 5
```

### `pecto pr-diff` — PR behavior diff

```bash
pecto pr-diff main HEAD    # outputs markdown to stdout
```

Generate a GitHub-flavored Markdown diff between two git refs. Use with the `pecto-pr` GitHub Action to auto-post behavior diffs on pull requests.

### `pecto domains` — Business domain clusters

```bash
pecto domains [path]
```

Groups capabilities by business domain (naming conventions + dependency analysis).

### `pecto graph` — Dependency graph

```bash
pecto graph [path] [--format text|dot|json]
```

Shows how capabilities depend on each other. DOT format for Graphviz, JSON for tooling.

### `pecto impact` — Change impact analysis

```bash
pecto impact <name> [--path .]
```

Traces the dependency graph to show what breaks if you change a capability.

### `pecto context` — AI/LLM context export

```bash
pecto context [path]
```

Compact project summary optimized for LLM context windows (~4K tokens).

### `pecto report` — HTML report

```bash
pecto report [path] [--output pecto-report.html]
```

Self-contained HTML with interactive D3.js dependency graph. Opens in any browser.

### `pecto serve` — Web dashboard

```bash
pecto serve [path] [--port 4321]
```

Live interactive dashboard at `http://localhost:4321` with dependency graph, domain clusters, and search.

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
| **JAX-RS Resources** | `@Path`, `@GET`/`@POST`/etc., `@PathParam`, `@QueryParam`, `@RolesAllowed`, `@PermitAll` |
| **JPA Entities** | `@Entity`, `@Table`, field types, `@Id`, `@Column`, relationships (`@OneToMany`, `@ManyToOne`) |
| **Spring Data Repos** | `JpaRepository<T, ID>`, CRUD operations, custom query methods, `@Query` |
| **JPA Repositories** | `@PersistenceContext EntityManager`, custom base repository classes |
| **Services** | `@Service`, `@Stateless`, `@RequestScoped`, `@Inject`, `@Transactional`, side effects |
| **Validation** | `@Valid` with cross-file DTO resolution: `@NotBlank`, `@Email`, `@Size`, `@Min`, `@Max`, `@Pattern` |
| **Security** | `@PreAuthorize`, `@Secured`, `@CrossOrigin`, `@RateLimiter`, `@RolesAllowed`, `@PermitAll` |
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

### Python

| Feature | What pecto extracts |
|---------|-------------------|
| **FastAPI** | `@router.get`/`@app.post`/etc., path params from type hints, `Depends()` security |
| **Flask** | `@app.route`/`@blueprint.route`, methods kwarg, path params |
| **Django REST** | `ModelViewSet` CRUD, `@api_view` function views |
| **SQLAlchemy** | `Column()`, `relationship()`, `__tablename__`, constraints |
| **Django Models** | `models.Model`, `CharField`, `ForeignKey`, `ManyToManyField` |
| **Pydantic** | `BaseModel`, `Field()` with min/max_length, gt/lt constraints |
| **Services** | `*Service`/`*Repository`/`*UseCase` classes, public methods |
| **Celery Tasks** | `@shared_task`, `@app.task`, `@periodic_task` |

### TypeScript / JavaScript

| Feature | What pecto extracts |
|---------|-------------------|
| **Express** | `router.get`/`app.post`/etc., `:param` path params, route strings |
| **NestJS** | `@Controller`, `@Get`/`@Post`/etc., `@UseGuards` security |
| **Next.js** | App Router `export function GET/POST` in route.ts, path from file structure |
| **TypeORM** | `@Entity`, `@Column`, `@PrimaryGeneratedColumn`, relationships |
| **Mongoose** | `new Schema({...})` detection |
| **Services** | `@Injectable`, `*Service`/`*Repository` classes, public methods |

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

### GitHub Action: `pecto-pr`

Auto-post behavior diffs on pull requests:

```yaml
# .github/workflows/pecto-pr.yml
on: pull_request
jobs:
  behavior-diff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pecto/pecto-pr@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

## Contributing

```bash
# Clone and build
git clone https://github.com/AliciaMartinelli/pecto
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
