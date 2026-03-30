# pecto Launch Plan

## Show HN Post

**Title:** pecto — A Rust tool that extracts behavior specs from Java, C#, Python, and TypeScript codebases in seconds

**Body:**

Hi HN,

I built pecto, a CLI tool that analyzes Java, C#, Python, and TypeScript codebases and generates structured behavior specifications using static analysis (tree-sitter, no LLMs).

The problem: Large codebases grow faster than documentation. You join a team and spend weeks understanding what the code actually does. AI tools can generate code but often can't explain existing behavior reliably.

pecto reads your source code and extracts:
- REST endpoints with paths, parameters, validation rules, and error behaviors
- Database entities with field types and constraints
- Service methods with side effects (DB writes, events, service calls)
- Security configuration (auth, roles, CORS, rate limits)
- Scheduled tasks and background services
- Request flow traces as Mermaid sequence diagrams (`pecto flow`)
- PR behavior diffs for code review (`pecto pr-diff`)
- Architecture fitness rule validation (`pecto check`)

It outputs YAML/JSON specs that are both human-readable and machine-verifiable. Supports Java (Spring, JAX-RS), C# (ASP.NET Core, EF Core), Python (FastAPI, Flask, Django), and TypeScript (NestJS, Express, Next.js).

Example: `pecto init ./my-spring-app` analyzes 86 Java files in ~0.5s and extracts 29 capabilities (8 entities, 7 repositories, 2 services, 109 operations).

Built in Rust for speed. Uses tree-sitter for parsing. No API keys, no cloud, no LLMs — pure static analysis.

Install: `cargo install pecto`

Website: https://pecto.dev
GitHub: https://github.com/AliciaMartinelli/pecto

Interested in feedback on:
1. Is behavior extraction useful for your workflow?
2. What language/framework should we support next (Go, PHP, Kotlin)?
3. Would you use `pecto verify` in CI to catch undocumented behavior changes?

---

## Benchmark Data

| Project | Language | Files | Capabilities | Time |
|---------|----------|-------|-------------|------|
| Spring PetClinic REST | Java | 86 | 29 (8 entities, 7 repos) | ~0.5s |
| eShopOnWeb | C# | 254 | 18 (7 entities, 25 endpoints) | ~0.5s |
| FastAPI Template | Python | 27 | 8 (23 endpoints, 20 entities) | ~0.4s |
| NestJS TypeORM | TypeScript | 7 | 3 (4 endpoints, 1 entity) | ~0.4s |

Hardware: Apple M-series, single run, cold cache.

## Reddit Posts

### r/java
"pecto — extract behavior specs from Spring Boot codebases (endpoints, entities, services, security) using Rust + tree-sitter. Open source."

### r/csharp / r/dotnet
"pecto — extract behavior specs from ASP.NET Core + Entity Framework projects. Controllers, entities, services, background tasks. Written in Rust, no LLMs."

### r/rust
"Built a behavior spec extractor for Java/C#/Python/TypeScript using tree-sitter. Parallel parsing with rayon. Now with request flow tracing and architecture fitness rules. Looking for feedback on the architecture."

### r/python
"pecto — extract behavior specs from FastAPI, Flask, and Django codebases (endpoints, SQLAlchemy/Django models, Pydantic validation, Celery tasks) using Rust + tree-sitter. Open source."

### r/typescript
"pecto — extract behavior specs from NestJS, Express, and Next.js codebases (endpoints, TypeORM entities, services, guards) using Rust + tree-sitter. Open source."

### r/node
"pecto — extract behavior specs from Express, NestJS, and Next.js apps. Endpoints, TypeORM/Mongoose models, services. Rust + tree-sitter static analysis, no LLMs. Open source."

### r/programming
"pecto — A tool that reads your code and tells you what it does. Static analysis for Java, C#, Python, and TypeScript — outputs YAML specs. Like Swagger but for all behavior, not just APIs."

## Feature Comparison

| Feature | pecto | Swagger/OpenAPI | SonarQube | CodeClimate |
|---------|-------|-----------------|-----------|-------------|
| REST endpoint extraction | Yes | Yes (annotations only) | No | No |
| Database entity extraction | Yes | No | No | No |
| Business logic / side effects | Yes | No | No | No |
| Security rules extraction | Yes | No | Partial | No |
| Scheduled task detection | Yes | No | No | No |
| Behavior drift detection | Yes (verify) | No | No | No |
| Git-based behavior diff | Yes (diff) | No | No | No |
| Request flow tracing | Yes (flow) | No | No | No |
| PR behavior diffs | Yes (pr-diff) | No | No | No |
| Architecture fitness rules | Yes (check) | No | No | No |
| Offline / no cloud | Yes | Yes | No | No |
| Multi-language (Java, C#, Python, TS) | Yes | Per-language | Yes | Yes |
| Speed | <1s for 250 files | N/A | Minutes | Minutes |
