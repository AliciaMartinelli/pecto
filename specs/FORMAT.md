# pecto Spec Format v0.2

The pecto spec format describes the **behavior** of software — what it does, under what conditions, and with what constraints.

## Design Principles

1. **Human-readable** — A non-technical stakeholder should understand the spec
2. **Machine-verifiable** — pecto can check if code matches the spec
3. **Language-agnostic** — The same format works for Java, C#, Python, TypeScript
4. **Auto-generated** — pecto generates specs from code; you refine them

## Top-Level Structure

```yaml
name: my-app
analyzed: '2026-03-25T10:00:00Z'
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

A **capability** groups related behaviors. Typically maps to a controller, service, entity, or module.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Kebab-case identifier (e.g., `user-authentication`) |
| `source` | string | Relative path to the primary source file |
| `endpoints` | list | HTTP endpoints (REST APIs) |
| `operations` | list | Non-HTTP operations (service methods, repository queries) |
| `entities` | list | Database entities |
| `scheduled_tasks` | list | Cron/scheduled jobs, event listeners, background tasks |

## Endpoint

An HTTP endpoint extracted from a controller.

### Java (Spring Boot)

```yaml
- method: POST
  path: /api/users
  input:
    body:
      name: CreateUserRequest
      fields:
        email: String
        name: String
  validation:
    - field: email
      constraints: ["@NotBlank", "@Email"]
    - field: name
      constraints: ["@NotBlank", "@Size(min=2, max=50)"]
  behaviors:
    - name: success
      returns:
        status: 201
        body:
          name: User
    - name: not-found
      returns:
        status: 404
  security:
    authentication: required
    roles: ["hasRole('ADMIN')"]
    cors: "origins: https://example.com"
    rate_limit: "rate-limiter: apiLimit"
```

### C# (ASP.NET Core)

```yaml
- method: POST
  path: api/users
  input:
    body:
      name: CreateUserRequest
      fields:
        Email: string
        Name: string
  validation:
    - field: Email
      constraints: ["[Required]", "[EmailAddress]"]
    - field: Name
      constraints: ["[Required]", "[StringLength(50)]"]
  behaviors:
    - name: success
      returns:
        status: 200
        body:
          name: User
    - name: not-found
      returns:
        status: 404
  security:
    authentication: required
    roles: ["Admin"]
```

## Behavior

A **behavior** describes what happens under specific conditions.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Descriptive name (e.g., `success`, `not-found`, `conflict`) |
| `condition` | string? | When this behavior occurs (e.g., `throws NotFoundException`) |
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

A database entity extracted from ORM annotations/attributes.

### Java (JPA/Hibernate)

```yaml
- name: Owner
  table: owners
  fields:
    - name: id
      type: Integer
      constraints: ["@Id", "@GeneratedValue"]
    - name: firstName
      type: String
      constraints: ["@NotBlank"]
    - name: pets
      type: List<Pet>
      constraints: ["@OneToMany"]
```

### C# (Entity Framework Core)

```yaml
- name: CatalogItem
  table: Catalog
  fields:
    - name: Id
      type: int
      constraints: ["[Key]"]
    - name: Name
      type: string
      constraints: ["[Required]", "[MaxLength(100)]"]
    - name: Price
      type: decimal
      constraints: ["[Required]"]
```

## Operation

A service method or repository operation.

```yaml
- name: createOrder
  source_method: OrderService#createOrder
  input:
    name: CreateOrderRequest
  transaction: required
  behaviors:
    - name: success
      returns:
        status: 200
        body:
          name: Order
      side_effects:
        - kind: db_insert
          table: orderRepository
        - kind: event
          name: OrderCreatedEvent
        - kind: call
          target: paymentService.charge
```

## Scheduled Task

```yaml
- name: hourlyCleanup
  schedule: "cron: 0 0 * * * *"
  description: null

- name: handleOrderCreated
  schedule: "event: OrderCreatedEvent"
  description: Handles OrderCreatedEvent events
```

## Versioning

The spec format follows semantic versioning. Breaking changes increment the major version.

Current version: **v0.2** (stabilizing)

### Changes from v0.1
- Added C# examples
- Added `cors` field to SecurityConfig
- Added Operation and ScheduledTask examples
- Clarified language-agnostic design with side-by-side Java/C# examples
