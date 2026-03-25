# pecto Launch Plan

## Show HN Post

**Title:** pecto — A Rust tool that extracts behavior specs from Java/C# codebases in seconds

**Body:**

Hi HN,

I built pecto, a CLI tool that analyzes Java and C# codebases and generates structured behavior specifications using static analysis (tree-sitter, no LLMs).

The problem: Large codebases grow faster than documentation. You join a team and spend weeks understanding what the code actually does. AI tools can generate code but often can't explain existing behavior reliably.

pecto reads your source code and extracts:
- REST endpoints with paths, parameters, validation rules, and error behaviors
- Database entities with field types and constraints
- Service methods with side effects (DB writes, events, service calls)
- Security configuration (auth, roles, CORS, rate limits)
- Scheduled tasks and background services

It outputs YAML/JSON specs that are both human-readable and machine-verifiable.

Example: `pecto init ./spring-petclinic-rest` analyzes 86 Java files in ~0.5s and finds 18 capabilities (8 entities, 7 repositories, 2 services, 1 controller).

Built in Rust for speed. Uses tree-sitter for parsing. No API keys, no cloud, no LLMs — pure static analysis.

GitHub: https://github.com/pecto-dev/pecto

Interested in feedback on:
1. Is behavior extraction useful for your workflow?
2. Which language/framework should we support next (Python/Django, TypeScript/NestJS)?
3. Would you use `pecto verify` in CI to catch undocumented behavior changes?

---

## Benchmark Data

| Project | Language | Files | Capabilities | Endpoints | Entities | Services | Time |
|---------|----------|-------|-------------|-----------|----------|----------|------|
| spring-petclinic-rest | Java | 86 | 18 | 1 | 8 | 2 | ~0.5s |
| eShopOnWeb | C# | 254 | 18 | 25 | 7 | 14 | ~1.6s |

Hardware: Apple M-series, single run, cold cache.

## Reddit Posts

### r/java
"pecto — extract behavior specs from Spring Boot codebases (endpoints, entities, services, security) using Rust + tree-sitter. Open source."

### r/csharp / r/dotnet
"pecto — extract behavior specs from ASP.NET Core + Entity Framework projects. Controllers, entities, services, background tasks. Written in Rust, no LLMs."

### r/rust
"Built a behavior spec extractor for Java/C# using tree-sitter-java and tree-sitter-c-sharp. Parallel parsing with rayon. Looking for feedback on the architecture."

### r/programming
"pecto — A tool that reads your code and tells you what it does. Static analysis for Java/C#, outputs YAML specs. Like Swagger but for all behavior, not just APIs."

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
| Offline / no cloud | Yes | Yes | No | No |
| Multi-language (Java + C#) | Yes | Per-language | Yes | Yes |
| Speed | <2s for 250 files | N/A | Minutes | Minutes |
