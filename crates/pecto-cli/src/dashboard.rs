use pecto_core::model::ProjectSpec;

/// Render the interactive HTML dashboard.
/// `is_live` adds a "live" badge (for serve mode).
pub fn render_html(spec: &ProjectSpec, is_live: bool) -> String {
    let spec_json = serde_json::to_string(spec).unwrap_or_default();
    let name = &spec.name;
    let files = spec.files_analyzed;
    let caps = spec.capabilities.len();
    let deps = spec.dependencies.len();
    let _ = is_live; // reserved for future live-reload feature
    let live_badge = "";

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>pecto — {name}</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #0f172a; color: #e2e8f0; }}
.header {{ padding: 14px 24px; border-bottom: 1px solid #1e293b; display: flex; align-items: center; gap: 16px; }}
.header h1 {{ font-size: 18px; font-family: monospace; font-weight: bold; background: linear-gradient(90deg, #C9C9EB, #E185C8, #DF0F51, #FFA161); -webkit-background-clip: text; background-clip: text; color: transparent; }}
.header .stats {{ color: #64748b; font-size: 13px; }}
.header .live {{ color: #34d399; font-size: 11px; margin-left: auto; }}
.container {{ display: grid; grid-template-columns: 1fr 360px; height: calc(100vh - 53px); }}
#graph {{ background: #0f172a; cursor: grab; position: relative; overflow: hidden; }}
#graph:active {{ cursor: grabbing; }}
#graph svg {{ position: absolute; top: 0; left: 0; width: 100%; height: 100%; }}
#domain-filter {{ pointer-events: auto; }}

/* Sidebar */
.sidebar {{ border-left: 1px solid #1e293b; overflow-y: auto; padding: 16px; }}
.search {{ width: 100%; padding: 8px 12px; background: #1e293b; border: 1px solid #334155; border-radius: 6px; color: #e2e8f0; font-size: 12px; margin-bottom: 12px; outline: none; }}
.search:focus {{ border-color: #E185C8; }}
.domain {{ margin-bottom: 12px; }}
.domain-header {{ font-size: 13px; font-weight: 600; padding: 6px 8px; background: #1e293b; border-radius: 4px; margin-bottom: 3px; }}
.cap-item {{ padding: 4px 8px; margin: 1px 0; border-radius: 3px; font-size: 11px; cursor: pointer; display: flex; justify-content: space-between; align-items: center; }}
.cap-item:hover {{ background: #1e293b; }}
.cap-item .badge {{ color: #64748b; font-size: 10px; }}
.cap-item .dot {{ width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; flex-shrink: 0; }}
.back-btn {{ display: block; padding: 6px 10px; margin-bottom: 12px; background: #1e293b; border: 1px solid #334155; border-radius: 4px; color: #94a3b8; font-size: 11px; cursor: pointer; text-align: left; }}
.back-btn:hover {{ background: #334155; }}
.detail-title {{ font-size: 16px; font-weight: 600; margin-bottom: 4px; }}
.detail-type {{ font-size: 11px; color: #64748b; margin-bottom: 12px; }}
.detail-section {{ margin-bottom: 12px; }}
.detail-section h4 {{ font-size: 11px; color: #64748b; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 4px; }}
.detail-item {{ font-size: 11px; color: #94a3b8; padding: 2px 0; }}
.detail-item .method {{ font-weight: 600; color: #E185C8; margin-right: 4px; }}
.detail-item .dep-arrow {{ color: #475569; }}

/* Tooltip */
.tooltip {{ position: fixed; background: #1e293b; border: 1px solid #334155; border-radius: 8px; padding: 10px 14px; font-size: 11px; color: #e2e8f0; pointer-events: none; z-index: 100; max-width: 280px; box-shadow: 0 4px 12px rgba(0,0,0,0.4); }}
.tooltip .tt-name {{ font-weight: 600; font-size: 13px; margin-bottom: 4px; }}
.tooltip .tt-type {{ color: #64748b; font-size: 10px; margin-bottom: 6px; }}
.tooltip .tt-stat {{ color: #94a3b8; }}
.tooltip .tt-file {{ color: #475569; font-size: 10px; margin-top: 4px; word-break: break-all; }}

/* Legend */
.legend {{ position: absolute; bottom: 16px; left: 16px; background: #1e293b; border: 1px solid #334155; border-radius: 8px; padding: 10px 14px; font-size: 10px; z-index: 10; }}
.legend-item {{ display: flex; align-items: center; gap: 6px; margin: 3px 0; color: #94a3b8; }}
.legend-dot {{ width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }}
.legend-line {{ width: 20px; height: 0; border-top: 2px solid #64748b; }}
.legend-line.dashed {{ border-top-style: dashed; }}
.legend-line.dotted {{ border-top-style: dotted; }}
.legend-title {{ font-weight: 600; color: #64748b; margin-bottom: 4px; text-transform: uppercase; letter-spacing: 0.05em; }}

/* Graph */
svg text {{ font-family: 'Inter', -apple-system, sans-serif; pointer-events: none; }}
.link {{ stroke-width: 1.5; fill: none; marker-end: url(#arrow); }}
.link.calls {{ stroke: #475569; }}
.link.queries {{ stroke: #475569; stroke-dasharray: 6,3; }}
.link.listens {{ stroke: #475569; stroke-dasharray: 2,3; }}
.link.validates {{ stroke: #475569; stroke-dasharray: 8,4,2,4; }}
.node circle {{ cursor: pointer; transition: opacity 0.2s; }}
.node text {{ transition: opacity 0.2s; }}
.node.dimmed circle {{ opacity: 0.15; }}
.node.dimmed text {{ opacity: 0.15; }}
.link.dimmed {{ opacity: 0.08; }}
.filter-btn {{ padding: 3px 8px; background: #1e293b; border: 1px solid #334155; border-radius: 4px; color: #94a3b8; font-size: 10px; cursor: pointer; }}
.filter-btn:hover {{ background: #334155; }}
.filter-btn.active {{ background: #334155; color: #E185C8; border-color: #E185C8; }}
</style>
</head>
<body>
<div class="header">
  <h1>pecto</h1>
  <div class="stats">{name} &mdash; {files} files, {caps} capabilities, {deps} dependencies</div>
  {live_badge}
</div>
<div class="container">
  <div id="graph" style="position:relative">
    <svg id="svg" style="width:100%;height:100%"></svg>
    <div class="legend">
      <div class="legend-title">Nodes</div>
      <div class="legend-item"><div class="legend-dot" style="background:#E185C8"></div> Controller</div>
      <div class="legend-item"><div class="legend-dot" style="background:#C9C9EB"></div> Entity</div>
      <div class="legend-item"><div class="legend-dot" style="background:#FFA161"></div> Repository</div>
      <div class="legend-item"><div class="legend-dot" style="background:#34d399"></div> Service</div>
      <div class="legend-item"><div class="legend-dot" style="background:#FBBF24"></div> Scheduled</div>
      <div class="legend-item"><div class="legend-dot" style="background:#818CF8"></div> DbContext</div>
      <div class="legend-title" style="margin-top:8px">Edges</div>
      <div class="legend-item"><div class="legend-line"></div> Calls</div>
      <div class="legend-item"><div class="legend-line dashed"></div> Queries</div>
      <div class="legend-item"><div class="legend-line dotted"></div> Listens</div>
    </div>
    <div id="domain-filter" style="position:absolute;top:8px;left:8px;z-index:10;display:flex;flex-wrap:wrap;gap:4px;align-items:center"></div>
    <div class="tooltip" id="tooltip" style="display:none"></div>
  </div>
  <div class="sidebar">
    <input class="search" id="search" placeholder="Search capabilities..." />
    <div id="sidebar-content"></div>
  </div>
</div>
<script>
const spec = {spec_json};

// Color + type mapping
function getCapType(cap) {{
  if (!cap) return {{ type: 'unknown', color: '#64748b' }};
  if (cap.endpoints?.length) return {{ type: 'Controller', color: '#E185C8' }};
  if (cap.name.includes('entity')) return {{ type: 'Entity', color: '#C9C9EB' }};
  if (cap.name.includes('repository')) return {{ type: 'Repository', color: '#FFA161' }};
  if (cap.name.includes('context')) return {{ type: 'DbContext', color: '#818CF8' }};
  if (cap.scheduled_tasks?.length) return {{ type: 'Scheduled', color: '#FBBF24' }};
  if (cap.operations?.length) return {{ type: 'Service', color: '#34d399' }};
  return {{ type: 'Other', color: '#64748b' }};
}}

function getCapSize(cap) {{
  if (!cap) return 8;
  const count = (cap.endpoints?.length || 0) + (cap.operations?.length || 0) + (cap.entities?.length || 0) + (cap.scheduled_tasks?.length || 0);
  return Math.min(20, Math.max(8, 6 + count * 1.5));
}}

// Sidebar rendering
const sidebarEl = document.getElementById('sidebar-content');
const searchEl = document.getElementById('search');

function renderOverview(filter) {{
  searchEl.style.display = '';
  let html = '';
  if (spec.domains?.length) {{
    spec.domains.forEach(d => {{
      const caps = filter ? d.capabilities.filter(c => c.includes(filter)) : d.capabilities;
      if (!caps.length) return;
      html += `<div class="domain"><div class="domain-header">${{d.name}} (${{caps.length}})</div>`;
      caps.forEach(c => {{
        const cap = spec.capabilities.find(x => x.name === c);
        const info = getCapType(cap);
        const badge = cap ? (cap.endpoints?.length ? cap.endpoints.length + ' ep' :
                             cap.entities?.length ? cap.entities.length + ' ent' :
                             cap.operations?.length ? cap.operations.length + ' ops' :
                             cap.scheduled_tasks?.length ? cap.scheduled_tasks.length + ' tasks' : '') : '';
        html += `<div class="cap-item" onclick="showDetail('${{c}}')"><span style="display:flex;align-items:center"><span class="dot" style="background:${{info.color}}"></span>${{c}}</span><span class="badge">${{badge}}</span></div>`;
      }});
      html += '</div>';
    }});
  }}
  const domainCaps = new Set((spec.domains || []).flatMap(d => d.capabilities));
  const orphans = spec.capabilities.filter(c => !domainCaps.has(c.name));
  const filtered = filter ? orphans.filter(c => c.name.includes(filter)) : orphans;
  if (filtered.length) {{
    html += '<div class="domain"><div class="domain-header">Other</div>';
    filtered.forEach(c => {{
      const info = getCapType(c);
      html += `<div class="cap-item" onclick="showDetail('${{c.name}}')"><span style="display:flex;align-items:center"><span class="dot" style="background:${{info.color}}"></span>${{c.name}}</span></div>`;
    }});
    html += '</div>';
  }}
  sidebarEl.innerHTML = html || '<div style="color:#475569;text-align:center;padding:20px">No results</div>';
}}

function showDetail(name) {{
  searchEl.style.display = 'none';
  const cap = spec.capabilities.find(c => c.name === name);
  if (!cap) return;
  const info = getCapType(cap);
  const incoming = (spec.dependencies || []).filter(d => d.to === name);
  const outgoing = (spec.dependencies || []).filter(d => d.from === name);

  let html = `<button class="back-btn" onclick="renderOverview('')">&larr; Back to overview</button>`;
  html += `<div class="detail-title" style="color:${{info.color}}">${{name}}</div>`;
  html += `<div class="detail-type">${{info.type}} &mdash; ${{cap.source}}</div>`;

  if (cap.endpoints?.length) {{
    html += `<div class="detail-section"><h4>Endpoints (${{cap.endpoints.length}})</h4>`;
    cap.endpoints.forEach(ep => {{
      const method = ep.method || '?';
      html += `<div class="detail-item"><span class="method">${{method}}</span> ${{ep.path}}`;
      if (ep.security?.authentication) html += ' &#128274;';
      html += `</div>`;
      if (ep.behaviors) {{
        ep.behaviors.filter(b => b.name !== 'success').forEach(b => {{
          html += `<div class="detail-item" style="padding-left:16px;color:#64748b">${{b.name}} &rarr; ${{b.returns?.status}}</div>`;
        }});
      }}
    }});
    html += '</div>';
  }}

  if (cap.entities?.length) {{
    html += `<div class="detail-section"><h4>Entities (${{cap.entities.length}})</h4>`;
    cap.entities.forEach(ent => {{
      html += `<div class="detail-item" style="font-weight:600">${{ent.name}} <span style="color:#475569">(${{ent.table}})</span></div>`;
      (ent.fields || []).forEach(f => {{
        const constraints = f.constraints?.length ? ' <span style="color:#64748b">' + f.constraints.join(', ') + '</span>' : '';
        html += `<div class="detail-item" style="padding-left:12px">${{f.name}}: <span style="color:#64748b">${{f.type || f.field_type || ''}}</span>${{constraints}}</div>`;
      }});
    }});
    html += '</div>';
  }}

  if (cap.operations?.length) {{
    html += `<div class="detail-section"><h4>Operations (${{cap.operations.length}})</h4>`;
    cap.operations.forEach(op => {{
      let extra = '';
      if (op.transaction) extra += ` <span style="color:#FBBF24">[tx:${{op.transaction}}]</span>`;
      html += `<div class="detail-item">${{op.name}}${{extra}}</div>`;
      if (op.behaviors) {{
        op.behaviors.forEach(b => {{
          (b.side_effects || []).forEach(se => {{
            const desc = se.table || se.name || se.description || se.target || '';
            html += `<div class="detail-item" style="padding-left:12px;color:#475569">&rarr; ${{se.kind || Object.keys(se)[0]}}: ${{desc}}</div>`;
          }});
        }});
      }}
    }});
    html += '</div>';
  }}

  if (cap.scheduled_tasks?.length) {{
    html += `<div class="detail-section"><h4>Scheduled Tasks</h4>`;
    cap.scheduled_tasks.forEach(t => {{
      html += `<div class="detail-item">${{t.name}} <span style="color:#64748b">(${{t.schedule}})</span></div>`;
    }});
    html += '</div>';
  }}

  if (outgoing.length) {{
    html += `<div class="detail-section"><h4>Depends on</h4>`;
    outgoing.forEach(d => {{
      html += `<div class="detail-item"><span class="dep-arrow">&rarr;</span> <span onclick="showDetail('${{d.to}}')" style="cursor:pointer;color:#E185C8">${{d.to}}</span> <span style="color:#475569">(${{d.kind}})</span></div>`;
    }});
    html += '</div>';
  }}

  if (incoming.length) {{
    html += `<div class="detail-section"><h4>Used by</h4>`;
    incoming.forEach(d => {{
      html += `<div class="detail-item"><span class="dep-arrow">&larr;</span> <span onclick="showDetail('${{d.from}}')" style="cursor:pointer;color:#E185C8">${{d.from}}</span> <span style="color:#475569">(${{d.kind}})</span></div>`;
    }});
    html += '</div>';
  }}

  sidebarEl.innerHTML = html;
}}

renderOverview('');
searchEl.addEventListener('input', e => renderOverview(e.target.value));

// Graph
const allDeps = spec.dependencies || [];
const svg = d3.select('#svg');
const graphEl = document.getElementById('graph');
const width = graphEl.clientWidth;
const height = graphEl.clientHeight;
svg.attr('viewBox', [0, 0, width, height]);

const tooltip = document.getElementById('tooltip');

// Build domain lookup
const domainOf = {{}};
(spec.domains || []).forEach(d => d.capabilities.forEach(c => domainOf[c] = d.name));
const domainNames = [...new Set(Object.values(domainOf))].sort();

// Domain filter buttons
const filterEl = document.getElementById('domain-filter');
if (domainNames.length > 1) {{
  let fhtml = '<span style="color:#64748b;font-size:10px;margin-right:6px">Filter:</span>';
  fhtml += `<button class="filter-btn active" onclick="filterDomain(null)">All</button>`;
  domainNames.forEach(d => {{
    fhtml += `<button class="filter-btn" onclick="filterDomain('${{d}}')">${{d}}</button>`;
  }});
  filterEl.innerHTML = fhtml;
}}

let activeDomain = null;

if (allDeps.length > 0) {{
  // Include ALL capabilities as nodes (not just those with deps)
  const depNodeIds = new Set(allDeps.flatMap(d => [d.from, d.to]));
  const allNodeIds = [...new Set([...depNodeIds, ...spec.capabilities.map(c => c.name)])];

  const nodes = allNodeIds.map(id => {{
    const cap = spec.capabilities.find(c => c.name === id);
    return {{ id, ...getCapType(cap), size: getCapSize(cap), domain: domainOf[id] || 'other' }};
  }});
  const links = allDeps.map(d => ({{ source: d.from, target: d.to, kind: d.kind || 'calls' }}));

  const g = svg.append('g');
  const zoomBehavior = d3.zoom().scaleExtent([0.1, 8]).on('zoom', (e) => g.attr('transform', e.transform));
  svg.call(zoomBehavior);

  // Domain cluster centers — must be computed before labels
  const domainCenters = {{}};
  const cols = Math.ceil(Math.sqrt(domainNames.length));
  domainNames.forEach((d, i) => {{
    domainCenters[d] = {{
      x: (i % cols + 0.5) * (width / cols),
      y: (Math.floor(i / cols) + 0.5) * (height / Math.ceil(domainNames.length / cols))
    }};
  }});

  // Keep domain centers within viewport with padding
  const pad = 80;
  Object.keys(domainCenters).forEach((d, i) => {{
    domainCenters[d] = {{
      x: pad + (i % cols + 0.5) * ((width - pad*2) / cols),
      y: pad + (Math.floor(i / cols) + 0.5) * ((height - pad*2) / Math.ceil(domainNames.length / cols))
    }};
  }});

  // Domain background labels
  const domainLabels = g.append('g').selectAll('text').data(domainNames).join('text')
    .attr('x', d => domainCenters[d]?.x || 0)
    .attr('y', d => (domainCenters[d]?.y || 0) - 10)
    .attr('text-anchor', 'middle')
    .attr('fill', '#1e293b')
    .attr('font-size', 18)
    .attr('font-weight', 700)
    .text(d => d);

  g.append('defs').append('marker')
    .attr('id', 'arrow').attr('viewBox', '0 -5 10 10')
    .attr('refX', 22).attr('refY', 0).attr('markerWidth', 6).attr('markerHeight', 6)
    .attr('orient', 'auto')
    .append('path').attr('d', 'M0,-5L10,0L0,5').attr('fill', '#475569');

  const sim = d3.forceSimulation(nodes)
    .force('link', d3.forceLink(links).id(d => d.id).distance(40).strength(0.1))
    .force('charge', d3.forceManyBody().strength(-60))
    .force('collision', d3.forceCollide().radius(d => d.size + 4))
    .force('cluster', d3.forceX(d => domainCenters[d.domain]?.x || width/2).strength(0.6))
    .force('clusterY', d3.forceY(d => domainCenters[d.domain]?.y || height/2).strength(0.6));

  const link = g.append('g').selectAll('line').data(links).join('line')
    .attr('class', d => 'link ' + d.kind);

  const node = g.append('g').selectAll('g').data(nodes).join('g').attr('class', 'node')
    .call(d3.drag()
      .on('start', (e, d) => {{ if (!e.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
      .on('drag', (e, d) => {{ d.fx = e.x; d.fy = e.y; }})
      .on('end', (e, d) => {{ if (!e.active) sim.alphaTarget(0); d.fx = null; d.fy = null; }}));

  node.append('circle')
    .attr('r', d => d.size)
    .attr('fill', d => d.color)
    .attr('stroke', '#1e293b')
    .attr('stroke-width', 2);

  // Only show labels for larger nodes (to reduce clutter)
  node.append('text').text(d => d.size >= 10 ? d.id : '').attr('x', d => d.size + 5).attr('y', 4)
    .attr('fill', '#94a3b8').attr('font-size', 10);

  // Domain filter function
  window.filterDomain = function(domain) {{
    activeDomain = domain;
    document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
    event.target.classList.add('active');
    node.style('display', d => (!domain || d.domain === domain) ? '' : 'none');
    link.style('display', d => {{
      if (!domain) return '';
      const sId = typeof d.source === 'object' ? d.source.id : d.source;
      const tId = typeof d.target === 'object' ? d.target.id : d.target;
      const sDomain = domainOf[sId];
      const tDomain = domainOf[tId];
      return (sDomain === domain || tDomain === domain) ? '' : 'none';
    }});
  }};

  // Hover: tooltip + highlight
  node.on('mouseenter', (event, d) => {{
    const cap = spec.capabilities.find(c => c.name === d.id);
    const info = getCapType(cap);
    const eps = cap?.endpoints?.length || 0;
    const ops = cap?.operations?.length || 0;
    const ents = cap?.entities?.length || 0;
    const inDeps = allDeps.filter(x => x.to === d.id).length;
    const outDeps = allDeps.filter(x => x.from === d.id).length;

    let tt = `<div class="tt-name">${{d.id}}</div>`;
    tt += `<div class="tt-type">${{info.type}}</div>`;
    if (eps) tt += `<div class="tt-stat">${{eps}} endpoints</div>`;
    if (ops) tt += `<div class="tt-stat">${{ops}} operations</div>`;
    if (ents) tt += `<div class="tt-stat">${{ents}} entities</div>`;
    if (outDeps) tt += `<div class="tt-stat">&rarr; ${{outDeps}} dependencies</div>`;
    if (inDeps) tt += `<div class="tt-stat">&larr; ${{inDeps}} dependents</div>`;
    if (cap?.source) tt += `<div class="tt-file">${{cap.source}}</div>`;

    tooltip.innerHTML = tt;
    tooltip.style.display = 'block';
    tooltip.style.left = (event.clientX + 14) + 'px';
    tooltip.style.top = (event.clientY - 10) + 'px';

    // Highlight connected
    const connected = new Set([d.id]);
    allDeps.forEach(dep => {{
      if (dep.from === d.id) connected.add(dep.to);
      if (dep.to === d.id) connected.add(dep.from);
    }});
    node.classed('dimmed', n => !connected.has(n.id));
    link.classed('dimmed', l => l.source.id !== d.id && l.target.id !== d.id);
  }})
  .on('mousemove', (event) => {{
    tooltip.style.left = (event.clientX + 14) + 'px';
    tooltip.style.top = (event.clientY - 10) + 'px';
  }})
  .on('mouseleave', () => {{
    tooltip.style.display = 'none';
    node.classed('dimmed', false);
    link.classed('dimmed', false);
  }})
  .on('click', (event, d) => {{
    showDetail(d.id);
  }});

  sim.on('tick', () => {{
    link.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
    node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
    domainLabels.attr('x', d => domainCenters[d]?.x || 0).attr('y', d => (domainCenters[d]?.y || 0) - 10);
  }});

  // Auto zoom-to-fit after 2 seconds
  setTimeout(() => {{
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    nodes.forEach(n => {{
      if (n.x < minX) minX = n.x;
      if (n.y < minY) minY = n.y;
      if (n.x > maxX) maxX = n.x;
      if (n.y > maxY) maxY = n.y;
    }});
    const bw = maxX - minX + 100;
    const bh = maxY - minY + 100;
    const scale = Math.min(width / bw, height / bh, 1.5) * 0.85;
    const tx = width / 2 - (minX + maxX) / 2 * scale;
    const ty = height / 2 - (minY + maxY) / 2 * scale;
    svg.transition().duration(800).call(
      zoomBehavior.transform, d3.zoomIdentity.translate(tx, ty).scale(scale)
    );
  }}, 2000);
}} else {{
  svg.append('text').attr('x', width/2).attr('y', height/2).attr('text-anchor', 'middle')
    .attr('fill', '#475569').attr('font-size', 14).text('No dependencies to visualize');
}}
</script>
</body>
</html>"##
    )
}
