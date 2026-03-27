# pecto V2: Deep Extraction + Visual Business Logic + PR Bot

> Vision: "Zeig mir visuell was mein Code tut, Schritt für Schritt."

---

## Phase A: Request Flow Extraction (Foundation)

### PR 1: Flow Model + Endpoint Flow Tracing (Java)
- [ ] `RequestFlow` + `FlowStep` + `FlowStepKind` im Core Model (`pecto-core/src/model.rs`)
- [ ] `flow.rs` in pecto-java: tracet Endpoint-Method → Service-Calls → rekursiv (Tiefe 4)
- [ ] Condition-Erkennung: `if`/`throw`/`return` als Verzweigungen
- [ ] `ProjectSpec.flows: Vec<RequestFlow>` befüllen
- [ ] Tests: Flow für Controller → Service → Repository Chain

### PR 2: Flow für C#, Python, TypeScript
- [ ] `flow.rs` für pecto-csharp (gleiche Logik, ASP.NET Patterns)
- [ ] `flow.rs` für pecto-python (FastAPI/Flask → Service Calls)
- [ ] `flow.rs` für pecto-typescript (Express/NestJS → Service Calls)
- [ ] Tests pro Sprache

### PR 3: `pecto flow <endpoint>` CLI Command
- [ ] Neuer Command: zeigt Flow als ASCII-Baum im Terminal
- [ ] Farbkodiert: grün=success, rot=error/throw, gelb=condition, blau=DB, magenta=event
- [ ] `--format text|mermaid|json` Flag
- [ ] Tests: CLI Output

---

## Phase B: Visual Business Logic (Dashboard + Diagrams)

### PR 4: Mermaid Sequence Diagram Generation
- [ ] `pecto-core/src/mermaid.rs`: Konvertiert `RequestFlow` → Mermaid Syntax
- [ ] Sequence Diagram mit Actors (Controller, Service, Repository, EventBus)
- [ ] `alt`/`else` Blöcke für Conditions
- [ ] `pecto flow <endpoint> --format mermaid` CLI Integration
- [ ] Tests: Mermaid Output validieren

### PR 5: Dashboard — Flow View (Sequence Diagram)
- [ ] Mermaid.js Integration im Dashboard HTML
- [ ] Klick auf Endpoint → Sequence Diagram erscheint in Sidebar/Overlay
- [ ] Flowchart-View für Service-Methoden (Decision Tree)
- [ ] Export-Button: Mermaid kopieren, SVG download

### PR 6: Dashboard — Architecture Flow Animation
- [ ] Klick auf Endpoint im Graph → Flow-Pfad wird highlighted
- [ ] Animierte Pfeile entlang des Pfads
- [ ] Nicht-beteiligte Nodes dimmen
- [ ] "Trace Mode" Toggle

---

## Phase C: GitHub PR Bot

### PR 7: `pecto pr-diff` Command
- [ ] Neuer Command: generiert GitHub-flavored Markdown
- [ ] Sections: New/Modified/Removed Endpoints, Entities, Dependencies
- [ ] Mermaid Architecture Diff Diagramm
- [ ] Flow-Changes: "Neuer Schritt in POST /orders: inventoryService.checkStock"
- [ ] Tests: Markdown Output

### PR 8: GitHub Action für PR Comments
- [ ] `.github/actions/pecto-pr/action.yml`
- [ ] Installiert pecto, führt `pecto pr-diff` aus
- [ ] Postet als PR-Kommentar via `github-script`
- [ ] Update-Modus: überschreibt vorherigen pecto-Kommentar statt neuen zu erstellen

---

## Phase D: Architecture Fitness Rules

### PR 9: Rule Engine + `pecto check`
- [ ] `.pecto/rules.yaml` Konfigurationsformat
- [ ] Built-in Rules:
  - `no-circular-dependencies`
  - `controllers-no-direct-db-access`
  - `all-endpoints-need-authentication`
  - `no-entity-without-validation`
  - `max-service-dependencies: 5`
- [ ] `pecto check` Command: validiert Regeln, Exit Code 1 bei Verletzung
- [ ] Farbige Terminal-Ausgabe: ✓ pass / ✗ fail pro Regel
- [ ] CI-Integration: `pecto check` in GitHub Action

---

## Reihenfolge

| Woche | Phase | PRs | Ergebnis |
|-------|-------|-----|----------|
| 1 | A | PR 1 (Flow Model + Java) | Foundation |
| 2 | A | PR 2 (Flow alle Sprachen) + PR 3 (CLI) | `pecto flow` funktioniert |
| 3 | B | PR 4 (Mermaid) + PR 5 (Dashboard Flow) | Visuelle Diagramme |
| 4 | B | PR 6 (Animation) | Graph-Animation |
| 5 | C | PR 7 (pr-diff) + PR 8 (GitHub Action) | PR Bot live |
| 6 | D | PR 9 (Rules Engine) | Architecture Compliance |
