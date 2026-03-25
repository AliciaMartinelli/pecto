use anyhow::{Context, Result};
use pecto_core::model::ProjectSpec;
use std::path::Path;

/// Generate a self-contained HTML report with embedded dependency graph.
pub fn generate_report(spec: &ProjectSpec, output: &Path) -> Result<()> {
    let spec_json = serde_json::to_string(spec).context("Failed to serialize spec for report")?;

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>pecto report — {name}</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #0f172a; color: #e2e8f0; }}
.header {{ padding: 24px 32px; border-bottom: 1px solid #1e293b; display: flex; align-items: center; gap: 16px; }}
.header h1 {{ font-size: 20px; color: #22d3ee; font-family: monospace; }}
.header .stats {{ color: #64748b; font-size: 14px; }}
.container {{ display: grid; grid-template-columns: 1fr 360px; height: calc(100vh - 73px); }}
#graph {{ background: #0f172a; }}
.sidebar {{ border-left: 1px solid #1e293b; overflow-y: auto; padding: 16px; }}
.sidebar h2 {{ font-size: 16px; margin-bottom: 12px; color: #94a3b8; }}
.sidebar h3 {{ font-size: 13px; color: #22d3ee; margin: 12px 0 6px; }}
.cap-item {{ padding: 6px 8px; margin: 2px 0; border-radius: 4px; font-size: 12px; cursor: pointer; }}
.cap-item:hover {{ background: #1e293b; }}
.cap-item .type {{ color: #64748b; font-size: 11px; }}
.domain {{ margin-bottom: 16px; }}
.domain-header {{ font-size: 14px; font-weight: 600; padding: 8px; background: #1e293b; border-radius: 6px; margin-bottom: 4px; }}
.dep {{ font-size: 11px; color: #64748b; padding: 2px 8px; }}
svg text {{ font-family: -apple-system, sans-serif; }}
.node circle {{ stroke: #1e293b; stroke-width: 2; }}
.link {{ stroke: #334155; stroke-width: 1.5; fill: none; marker-end: url(#arrow); }}
</style>
</head>
<body>
<div class="header">
  <h1>pecto</h1>
  <div class="stats">{name} — {files} files, {caps} capabilities, {deps} dependencies</div>
</div>
<div class="container">
  <svg id="graph"></svg>
  <div class="sidebar" id="sidebar"></div>
</div>
<script>
const spec = {spec_json};

// Sidebar
const sidebar = document.getElementById('sidebar');
let html = '<h2>Domains</h2>';
if (spec.domains && spec.domains.length > 0) {{
  spec.domains.forEach(d => {{
    html += `<div class="domain"><div class="domain-header">${{d.name}} (${{d.capabilities.length}})</div>`;
    d.capabilities.forEach(c => {{
      const cap = spec.capabilities.find(x => x.name === c);
      const type = cap ? (cap.endpoints?.length ? 'endpoints: ' + cap.endpoints.length :
                          cap.entities?.length ? 'entities: ' + cap.entities.length :
                          cap.operations?.length ? 'operations: ' + cap.operations.length : '') : '';
      html += `<div class="cap-item">${{c}} <span class="type">${{type}}</span></div>`;
    }});
    if (d.external_dependencies?.length) {{
      html += `<div class="dep">depends on: ${{d.external_dependencies.join(', ')}}</div>`;
    }}
    html += '</div>';
  }});
}} else {{
  spec.capabilities.forEach(c => {{
    html += `<div class="cap-item">${{c.name}}</div>`;
  }});
}}
sidebar.innerHTML = html;

// Graph
const deps = spec.dependencies || [];
if (deps.length === 0) {{
  document.getElementById('graph').innerHTML = '<text x="50%" y="50%" text-anchor="middle" fill="#64748b" font-size="16">No dependencies to visualize</text>';
}} else {{
  const nodes = [...new Set(deps.flatMap(d => [d.from, d.to]))].map(id => ({{ id }}));
  const links = deps.map(d => ({{ source: d.from, target: d.to, kind: d.kind }}));

  const svg = d3.select('#graph');
  const width = svg.node().getBoundingClientRect().width;
  const height = svg.node().getBoundingClientRect().height;

  svg.attr('viewBox', [0, 0, width, height]);

  svg.append('defs').append('marker')
    .attr('id', 'arrow').attr('viewBox', '0 -5 10 10')
    .attr('refX', 20).attr('refY', 0).attr('markerWidth', 6).attr('markerHeight', 6)
    .attr('orient', 'auto')
    .append('path').attr('d', 'M0,-5L10,0L0,5').attr('fill', '#475569');

  const simulation = d3.forceSimulation(nodes)
    .force('link', d3.forceLink(links).id(d => d.id).distance(120))
    .force('charge', d3.forceManyBody().strength(-300))
    .force('center', d3.forceCenter(width / 2, height / 2));

  const link = svg.append('g').selectAll('line').data(links).join('line').attr('class', 'link');

  const colors = {{ calls: '#22d3ee', queries: '#a78bfa', listens: '#34d399', validates: '#fbbf24' }};

  const node = svg.append('g').selectAll('g').data(nodes).join('g')
    .call(d3.drag().on('start', (e, d) => {{ if (!e.active) simulation.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
                   .on('drag', (e, d) => {{ d.fx = e.x; d.fy = e.y; }})
                   .on('end', (e, d) => {{ if (!e.active) simulation.alphaTarget(0); d.fx = null; d.fy = null; }}));

  node.append('circle').attr('r', 8).attr('fill', d => {{
    const cap = spec.capabilities.find(c => c.name === d.id);
    if (cap?.endpoints?.length) return '#22d3ee';
    if (cap?.entities?.length) return '#a78bfa';
    if (cap?.operations?.length) return '#34d399';
    return '#64748b';
  }});

  node.append('text').text(d => d.id).attr('x', 12).attr('y', 4).attr('fill', '#94a3b8').attr('font-size', 11);

  simulation.on('tick', () => {{
    link.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
    node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
  }});
}}
</script>
</body>
</html>"##,
        name = spec.name,
        files = spec.files_analyzed,
        caps = spec.capabilities.len(),
        deps = spec.dependencies.len(),
        spec_json = spec_json,
    );

    std::fs::write(output, html)
        .with_context(|| format!("Failed to write report to {}", output.display()))?;

    Ok(())
}
