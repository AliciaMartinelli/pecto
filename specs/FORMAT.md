# pecto Spec Format v0.1

The pecto spec format describes the **behavior** of software — what it does, under what conditions, and with what constraints.

## Design Principles

1. **Human-readable** — A non-technical stakeholder should understand the spec
2. **Machine-verifiable** — pecto can check if code matches the spec
3. **Language-agnostic** — The same format works for Java, C#, Python, TypeScript
4. **Auto-generated** — pecto generates specs from code; you refine them

## Top-Level Structure

```yaml
project: my-app
analyzed: 2026-03-25T10:00:00Z
files_analyzed: 247
capabilities:
  - name: user-authentication
    source: src/main/java/com/app/auth/AuthController.java
    endpoints: [...]
    operations: [...]
    entities: [...]
    scheduled_tasks: [...]
```

## Capability

A **capability** groups related behaviors. Typically maps to a controller, service, or module.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Kebab-case identifier (e.g., `user-authentication`) |
| `source` | string | Relative path to the primary source file |
| `endpoints` | list | HTTP endpoints (REST APIs) |
| `operations` | list | Non-HTTP operations (service methods) |
| `entities` | list | Database entities |
| `scheduled_tasks` | list | Cron/scheduled jobs |

## Endpoint

An HTTP endpoint extracted from a controller.

```yaml
endpoint:
  method: POST
  path: /api/auth/login
  input:
    body:
      name: LoginRequest
      fields:
        email: String
        password: String
    path_params: []
    query_params: []
  validation:
    - field: email
      constraints: ["@NotBlank", "@Email"]
  behaviors:
    - name: success
      returns:
        status: 200
        body:
          name: AuthResponse
          fields: {token: String}
      side_effects:
        - kind: db_insert
          table: audit_log
    - name: invalid_credentials
      condition: wrong password
      returns:
        status: 401
  security:
    authentication: JWT Bearer Token
    roles: ["ROLE_USER"]
    rate_limit: 20/min per IP
```

## Behavior

A **behavior** describes what happens under specific conditions.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Descriptive name (e.g., `success`, `not-found`) |
| `condition` | string? | When this behavior occurs |
| `returns` | ResponseSpec | HTTP status + response body |
| `side_effects` | list | Database writes, events, service calls |

## Side Effects

| Kind | Fields | Example |
|------|--------|---------|
| `db_insert` | `table` | Insert row into audit_log |
| `db_update` | `description` | Increment failed attempts |
| `event` | `name` | Publish OrderCreatedEvent |
| `call` | `target` | Call inventoryService.reserveItems |

## Entity

A database entity extracted from ORM annotations.

```yaml
entity:
  name: User
  table: users
  fields:
    - name: id
      type: Long
      constraints: ["@Id", "@GeneratedValue"]
    - name: email
      type: String
      constraints: ["@Column(unique=true)", "@NotBlank"]
```

## Versioning

The spec format follows semantic versioning. Breaking changes increment the major version.

Current version: **v0.1** (unstable, subject to change)
