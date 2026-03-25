use anyhow::{Context, Result};
use axum::{Json, Router, extract::State, response::Html, routing::get};
use pecto_core::model::ProjectSpec;
use std::sync::Arc;

/// Start a local web server serving the interactive pecto dashboard.
pub fn serve(spec: ProjectSpec, port: u16) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
    rt.block_on(async {
        let state = Arc::new(spec);

        let app = Router::new()
            .route("/", get(index_handler))
            .route("/api/spec", get(spec_handler))
            .with_state(state);

        let addr = format!("0.0.0.0:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("Failed to bind to {}", addr))?;

        eprintln!(
            "  Dashboard: http://localhost:{}\n  Press Ctrl+C to stop\n",
            port
        );

        axum::serve(listener, app).await.context("Server error")?;

        Ok(())
    })
}

async fn spec_handler(State(spec): State<Arc<ProjectSpec>>) -> Json<ProjectSpec> {
    Json((*spec).clone())
}

async fn index_handler(State(spec): State<Arc<ProjectSpec>>) -> Html<String> {
    let spec_json = serde_json::to_string(&*spec).unwrap_or_default();
    let name = &spec.name;
    let files = spec.files_analyzed;
    let caps = spec.capabilities.len();
    let deps = spec.dependencies.len();

    Html(format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>pecto dashboard — {name}</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #0f172a; color: #e2e8f0; }}
.header {{ padding: 16px 24px; border-bottom: 1px solid #1e293b; display: flex; align-items: center; gap: 16px; }}
.header h1 {{ font-size: 18px; font-family: monospace; font-weight: bold; background: linear-gradient(90deg, #C9C9EB, #E185C8, #DF0F51, #FFA161); -webkit-background-clip: text; background-clip: text; color: transparent; }}
.header .stats {{ color: #64748b; font-size: 13px; }}
.header .live {{ color: #34d399; font-size: 11px; margin-left: auto; }}
.container {{ display: grid; grid-template-columns: 1fr 340px; height: calc(100vh - 57px); }}
#graph {{ background: #0f172a; cursor: grab; }}
#graph:active {{ cursor: grabbing; }}
.sidebar {{ border-left: 1px solid #1e293b; overflow-y: auto; padding: 16px; }}
.sidebar h2 {{ font-size: 14px; margin-bottom: 8px; color: #94a3b8; }}
.domain {{ margin-bottom: 12px; }}
.domain-header {{ font-size: 13px; font-weight: 600; padding: 6px 8px; background: #1e293b; border-radius: 4px; margin-bottom: 3px; cursor: pointer; }}
.domain-header:hover {{ background: #334155; }}
.cap-item {{ padding: 4px 8px; margin: 1px 0; border-radius: 3px; font-size: 11px; cursor: pointer; display: flex; justify-content: space-between; }}
.cap-item:hover {{ background: #1e293b; }}
.cap-item .badge {{ color: #64748b; font-size: 10px; }}
.dep-line {{ font-size: 10px; color: #475569; padding: 1px 8px; }}
.search {{ width: 100%; padding: 8px 12px; background: #1e293b; border: 1px solid #334155; border-radius: 6px; color: #e2e8f0; font-size: 12px; margin-bottom: 12px; outline: none; }}
.search:focus {{ border-color: #22d3ee; }}
svg text {{ font-family: -apple-system, sans-serif; pointer-events: none; }}
.link {{ stroke: #334155; stroke-width: 1.5; fill: none; marker-end: url(#arrow); }}
.node circle {{ cursor: pointer; transition: r 0.2s; }}
.node circle:hover {{ r: 12; }}
</style>
</head>
<body>
<div class="header">
  <h1>pecto</h1>
  <div class="stats">{name} &mdash; {files} files, {caps} capabilities, {deps} deps</div>
  <div class="live">&#9679; live</div>
</div>
<div class="container">
  <svg id="graph"></svg>
  <div class="sidebar">
    <input class="search" id="search" placeholder="Search capabilities..." />
    <div id="sidebar-content"></div>
  </div>
</div>
<script>
const spec = {spec_json};
const sidebar = document.getElementById('sidebar-content');

function renderSidebar(filter) {{
  let html = '';
  if (spec.domains && spec.domains.length > 0) {{
    spec.domains.forEach(d => {{
      const caps = filter ? d.capabilities.filter(c => c.includes(filter)) : d.capabilities;
      if (caps.length === 0) return;
      html += `<div class="domain"><div class="domain-header">${{d.name}} (${{caps.length}})</div>`;
      caps.forEach(c => {{
        const cap = spec.capabilities.find(x => x.name === c);
        const badge = cap ? (cap.endpoints?.length ? cap.endpoints.length + ' ep' :
                             cap.entities?.length ? cap.entities.length + ' ent' :
                             cap.operations?.length ? cap.operations.length + ' ops' : '') : '';
        html += `<div class="cap-item"><span>${{c}}</span><span class="badge">${{badge}}</span></div>`;
      }});
      html += '</div>';
    }});
  }}
  const domainCaps = new Set((spec.domains || []).flatMap(d => d.capabilities));
  const orphans = spec.capabilities.filter(c => !domainCaps.has(c.name));
  const filtered = filter ? orphans.filter(c => c.name.includes(filter)) : orphans;
  if (filtered.length > 0) {{
    html += '<div class="domain"><div class="domain-header">Other</div>';
    filtered.forEach(c => {{
      html += `<div class="cap-item">${{c.name}}</div>`;
    }});
    html += '</div>';
  }}
  sidebar.innerHTML = html || '<div style="color:#475569;text-align:center;padding:20px">No results</div>';
}}
renderSidebar('');
document.getElementById('search').addEventListener('input', e => renderSidebar(e.target.value));

// Graph
const deps = spec.dependencies || [];
const svg = d3.select('#graph');
const width = svg.node().getBoundingClientRect().width;
const height = svg.node().getBoundingClientRect().height;
svg.attr('viewBox', [0, 0, width, height]);

if (deps.length > 0) {{
  const nodes = [...new Set(deps.flatMap(d => [d.from, d.to]))].map(id => ({{ id }}));
  const links = deps.map(d => ({{ source: d.from, target: d.to }}));

  svg.append('defs').append('marker')
    .attr('id', 'arrow').attr('viewBox', '0 -5 10 10')
    .attr('refX', 20).attr('refY', 0).attr('markerWidth', 6).attr('markerHeight', 6)
    .attr('orient', 'auto')
    .append('path').attr('d', 'M0,-5L10,0L0,5').attr('fill', '#475569');

  const sim = d3.forceSimulation(nodes)
    .force('link', d3.forceLink(links).id(d => d.id).distance(100))
    .force('charge', d3.forceManyBody().strength(-250))
    .force('center', d3.forceCenter(width / 2, height / 2))
    .force('collision', d3.forceCollide().radius(30));

  const link = svg.append('g').selectAll('line').data(links).join('line').attr('class', 'link');
  const node = svg.append('g').selectAll('g').data(nodes).join('g').attr('class', 'node')
    .call(d3.drag()
      .on('start', (e, d) => {{ if (!e.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
      .on('drag', (e, d) => {{ d.fx = e.x; d.fy = e.y; }})
      .on('end', (e, d) => {{ if (!e.active) sim.alphaTarget(0); d.fx = null; d.fy = null; }}));

  node.append('circle').attr('r', 8).attr('fill', d => {{
    const c = spec.capabilities.find(x => x.name === d.id);
    if (c?.endpoints?.length) return '#22d3ee';
    if (c?.entities?.length) return '#a78bfa';
    if (c?.operations?.length) return '#34d399';
    return '#64748b';
  }}).attr('stroke', '#1e293b').attr('stroke-width', 2);

  node.append('text').text(d => d.id).attr('x', 12).attr('y', 4).attr('fill', '#94a3b8').attr('font-size', 11);

  sim.on('tick', () => {{
    link.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
    node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
  }});
}} else {{
  svg.append('text').attr('x', width/2).attr('y', height/2).attr('text-anchor', 'middle')
    .attr('fill', '#475569').attr('font-size', 14).text('No dependencies to visualize');
}}
</script>
</body>
</html>"##
    ))
}
