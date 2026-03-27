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
<script src="https://cdn.jsdelivr.net/npm/mermaid@10.9.3/dist/mermaid.min.js"></script>
<script>mermaid.initialize({{ startOnLoad: false, theme: 'dark' }});</script>
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
.flow-container {{ margin-top: 12px; background: #0f172a; border: 1px solid #334155; border-radius: 8px; padding: 12px; overflow-x: auto; }}
.flow-container .mermaid {{ background: transparent; }}
.flow-header {{ display: flex; justify-content: space-between; align-items: center; margin-top: 16px; margin-bottom: 8px; }}
.flow-header h4 {{ font-size: 11px; color: #64748b; text-transform: uppercase; letter-spacing: 0.05em; }}
.copy-btn {{ padding: 3px 8px; background: #1e293b; border: 1px solid #334155; border-radius: 4px; color: #94a3b8; font-size: 10px; cursor: pointer; }}
.copy-btn:hover {{ background: #334155; }}
.flow-overlay {{ position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background: rgba(15,23,42,0.97); z-index: 200; display: flex; flex-direction: column; }}
.flow-overlay-header {{ padding: 16px 24px; display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid #1e293b; }}
.flow-overlay-header h2 {{ font-size: 16px; color: #e2e8f0; font-weight: 600; }}
.flow-overlay-body {{ flex: 1; min-height: 0; position: relative; overflow: hidden; }}
.flow-info-panel {{ position: absolute; left: 0; top: 0; bottom: 0; width: 320px; border-right: 1px solid #1e293b; padding: 20px; overflow-y: auto; background: inherit; z-index: 1; }}
.flow-info-section {{ margin-bottom: 16px; }}
.flow-info-section h4 {{ font-size: 10px; color: #64748b; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 6px; }}
.flow-info-entry {{ font-size: 12px; color: #e2e8f0; font-family: monospace; margin-bottom: 2px; word-break: break-word; }}
.flow-info-field {{ font-size: 11px; color: #94a3b8; padding-left: 10px; margin-bottom: 1px; font-family: monospace; }}
.flow-info-field .fi-name {{ color: #C9C9EB; }}
.flow-info-field .fi-type {{ color: #64748b; }}
.flow-info-badge {{ display: inline-block; padding: 2px 6px; border-radius: 3px; font-size: 10px; font-weight: 600; margin-right: 4px; margin-bottom: 4px; }}
.flow-info-badge.auth {{ background: #1e293b; color: #FBBF24; border: 1px solid #FBBF24; }}
.flow-info-badge.role {{ background: #1e293b; color: #FFA161; border: 1px solid #FFA161; }}
.flow-info-response {{ font-size: 11px; color: #94a3b8; display: flex; justify-content: space-between; padding: 3px 0; border-bottom: 1px solid rgba(30,41,59,0.3); }}
.flow-info-status-2xx {{ color: #34d399; }}
.flow-info-status-4xx {{ color: #FBBF24; }}
.flow-info-status-5xx {{ color: #DF0F51; }}
.flow-info-constraint {{ font-size: 10px; color: #E185C8; padding-left: 10px; margin-bottom: 2px; }}
.flow-diagram-area {{ position: absolute; left: 320px; top: 0; right: 0; bottom: 0; overflow: auto; padding: 24px; }}
.flow-overlay-actions {{ display: flex; gap: 8px; }}
.close-btn {{ padding: 6px 14px; background: #334155; border: 1px solid #475569; border-radius: 6px; color: #e2e8f0; font-size: 12px; cursor: pointer; }}
.close-btn:hover {{ background: #475569; }}
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
    <div id="flow-overlay" class="flow-overlay" style="display:none"></div>
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

// Newline char (avoids escape-sequence issues in embedded JS)
const NL = String.fromCharCode(10);

// Sanitize text for Mermaid diagram labels (remove syntax-breaking chars)
function mermaidSafe(text) {{
  var clean = text.replace(/[";#<>{{}}]/g, '').replace(/:/g, ' ');
  return clean.length > 100 ? clean.substring(0, 97) + '...' : clean;
}}

// Convert a flow to Mermaid sequence diagram (JS version)
function flowToMermaid(flow) {{
  const lines = ['sequenceDiagram'];
  const actors = new Set(['Client']);
  function collectActors(steps) {{
    steps.forEach(s => {{
      if (s.actor) actors.add(s.actor.replace(/[^a-zA-Z0-9_]/g, '_'));
      if (s.children) collectActors(s.children);
    }});
  }}
  collectActors(flow.steps);
  actors.forEach(a => lines.push('    participant ' + a));

  let lastActor = flow.steps.length > 0 && flow.steps[0].actor ? flow.steps[0].actor.replace(/[^a-zA-Z0-9_]/g, '_') : 'Server';
  lines.push('    Client->>' + lastActor + ': ' + mermaidSafe(flow.trigger));

  function renderSteps(steps) {{
    steps.forEach(step => {{
      const actor = step.actor ? step.actor.replace(/[^a-zA-Z0-9_]/g, '_') : lastActor;
      const desc = mermaidSafe(step.description);
      switch(step.kind) {{
        case 'service_call':
          lines.push('    ' + lastActor + '->>' + actor + ': ' + desc);
          if (step.children && step.children.length) renderSteps(step.children);
          lastActor = actor;
          break;
        case 'db_write':
          lines.push('    ' + lastActor + '->>' + actor + ': ' + desc);
          break;
        case 'db_read':
          lines.push('    ' + lastActor + '->>' + actor + ': ' + desc);
          lines.push('    ' + actor + '-->>' + lastActor + ': result');
          break;
        case 'event_publish':
          lines.push('    ' + lastActor + '->>EventBus: ' + desc);
          break;
        case 'validation':
          lines.push('    Note over ' + lastActor + ': ' + desc);
          break;
        case 'security_guard':
          lines.push('    Note over ' + lastActor + ': ' + desc);
          break;
        case 'condition':
          var cond = step.condition || step.description;
          var safeCond = mermaidSafe(cond);
          if (cond === 'else') {{
            lines.push('    else ' + safeCond);
          }} else {{
            lines.push('    alt ' + safeCond);
          }}
          if (step.children && step.children.length) renderSteps(step.children);
          if (cond !== 'else') lines.push('    end');
          break;
        case 'throw_exception':
          lines.push('    ' + lastActor + '->>Client: Error ' + desc);
          break;
      }}
    }});
  }}
  renderSteps(flow.steps);
  lines.push('    ' + lastActor + '->>Client: Response');
  return lines.join(NL);
}}

// Find the endpoint matching a flow trigger (e.g. "Post /api/users")
function findEndpointForFlow(flow) {{
  var parts = flow.trigger.split(' ');
  var triggerMethod = parts[0].toLowerCase();
  var triggerPath = parts.slice(1).join(' ').toLowerCase();
  // Exact method+path match
  for (var i = 0; i < spec.capabilities.length; i++) {{
    var eps = spec.capabilities[i].endpoints || [];
    for (var j = 0; j < eps.length; j++) {{
      if (eps[j].path.toLowerCase() === triggerPath
          && eps[j].method.toLowerCase() === triggerMethod) return eps[j];
    }}
  }}
  // Fallback: path-only match
  for (var i = 0; i < spec.capabilities.length; i++) {{
    var eps = spec.capabilities[i].endpoints || [];
    for (var j = 0; j < eps.length; j++) {{
      if (eps[j].path.toLowerCase() === triggerPath) return eps[j];
    }}
  }}
  return null;
}}

// Build HTML for the info panel
function buildFlowInfoPanel(flow, ep) {{
  var h = '';
  // Entry point (shorten: extract filename#name from full path)
  var epDisplay = flow.entry_point || '';
  var hashIdx = epDisplay.lastIndexOf('#');
  if (hashIdx > 0) {{
    var filePart = epDisplay.substring(0, hashIdx);
    var slashIdx = filePart.lastIndexOf('/');
    if (slashIdx >= 0) filePart = filePart.substring(slashIdx + 1);
    epDisplay = filePart + epDisplay.substring(hashIdx);
  }}
  h += '<div class="flow-info-section"><h4>Entry Point</h4>';
  h += '<div class="flow-info-entry">' + epDisplay + '</div></div>';

  // Security
  if (ep && ep.security) {{
    var sec = ep.security;
    h += '<div class="flow-info-section"><h4>Security</h4>';
    if (sec.authentication && sec.authentication !== 'required') h += '<span class="flow-info-badge auth">' + sec.authentication + '</span>';
    if (sec.roles && sec.roles.length) {{
      sec.roles.forEach(function(r) {{ h += '<span class="flow-info-badge role">' + r + '</span>'; }});
    }}
    if (sec.rate_limit) h += '<div class="flow-info-field">Rate limit: ' + sec.rate_limit + '</div>';
    h += '</div>';
  }}

  if (ep && ep.input) {{
    var inp = ep.input;
    // Path params
    if (inp.path_params && inp.path_params.length) {{
      h += '<div class="flow-info-section"><h4>Path Params</h4>';
      inp.path_params.forEach(function(p) {{
        h += '<div class="flow-info-field"><span class="fi-name">' + p.name + '</span>: <span class="fi-type">' + (p.param_type || '') + '</span></div>';
      }});
      h += '</div>';
    }}
    // Query params
    if (inp.query_params && inp.query_params.length) {{
      h += '<div class="flow-info-section"><h4>Query Params</h4>';
      inp.query_params.forEach(function(p) {{
        h += '<div class="flow-info-field"><span class="fi-name">' + p.name + '</span>: <span class="fi-type">' + (p.param_type || '') + '</span>'
          + (p.required ? '' : ' <span style="color:#475569">(opt)</span>') + '</div>';
      }});
      h += '</div>';
    }}
    // Request body
    if (inp.body) {{
      h += '<div class="flow-info-section"><h4>Request Body</h4>';
      h += '<div class="flow-info-entry">' + inp.body.name + '</div>';
      if (inp.body.fields) {{
        Object.keys(inp.body.fields).forEach(function(k) {{
          h += '<div class="flow-info-field"><span class="fi-name">' + k + '</span>: <span class="fi-type">' + inp.body.fields[k] + '</span></div>';
        }});
      }}
      h += '</div>';
    }}
  }}

  // Validation
  if (ep && ep.validation && ep.validation.length) {{
    h += '<div class="flow-info-section"><h4>Validation</h4>';
    ep.validation.forEach(function(v) {{
      h += '<div class="flow-info-constraint">' + v.field + ': ' + (v.constraints ? v.constraints.join(', ') : '') + '</div>';
    }});
    h += '</div>';
  }}

  // Responses / Behaviors
  if (ep && ep.behaviors && ep.behaviors.length) {{
    h += '<div class="flow-info-section"><h4>Responses</h4>';
    ep.behaviors.forEach(function(b) {{
      var st = b.returns ? b.returns.status : 0;
      var cls = st < 300 ? 'flow-info-status-2xx' : (st < 500 ? 'flow-info-status-4xx' : 'flow-info-status-5xx');
      h += '<div class="flow-info-response"><span>' + b.name + '</span><span class="' + cls + '">' + st + '</span></div>';
      if (b.returns && b.returns.body) h += '<div class="flow-info-field" style="font-size:10px">&rarr; ' + b.returns.body.name + '</div>';
    }});
    h += '</div>';
  }}

  return h;
}}

// Store mermaid code for copy button
let currentMermaidCode = '';
let mermaidRenderCounter = 0;

// Flow overlay
async function showFlowOverlay(flowIdx) {{
  const flow = spec.flows[flowIdx];
  if (!flow) return;
  currentMermaidCode = flowToMermaid(flow);
  console.log('Mermaid code:', currentMermaidCode);
  var endpoint = findEndpointForFlow(flow);
  var infoHtml = buildFlowInfoPanel(flow, endpoint);
  const overlay = document.getElementById('flow-overlay');
  overlay.innerHTML = '<div class="flow-overlay-header">'
    + '<h2>' + flow.trigger + '</h2>'
    + '<div class="flow-overlay-actions">'
    + '<button class="copy-btn" onclick="navigator.clipboard.writeText(currentMermaidCode).then(function(){{ event.target.textContent=\'Copied!\' }})">Copy Mermaid</button>'
    + '<button class="close-btn" onclick="document.getElementById(\'flow-overlay\').style.display=\'none\'">Close</button>'
    + '</div></div>'
    + '<div class="flow-overlay-body">'
    + '<div class="flow-info-panel">' + infoHtml + '</div>'
    + '<div class="flow-diagram-area"><div id="mermaid-target"></div></div>'
    + '</div>';
  overlay.style.display = 'flex';
  mermaidRenderCounter++;
  const renderId = 'mermaid-render-' + mermaidRenderCounter;
  try {{
    const result = await mermaid.render(renderId, currentMermaidCode);
    document.getElementById('mermaid-target').innerHTML = result.svg;
  }} catch(e) {{
    console.error('Mermaid render error:', e);
    var pre = document.createElement('pre');
    pre.style.cssText = 'color:#94a3b8;font-size:12px;white-space:pre-wrap;';
    pre.textContent = currentMermaidCode;
    document.getElementById('mermaid-target').appendChild(pre);
  }}
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

      // Security details
      if (ep.security) {{
        const sec = ep.security;
        if (sec.roles?.length) html += `<div class="detail-item" style="padding-left:16px;color:#FBBF24">roles: ${{sec.roles.join(', ')}}</div>`;
        if (sec.cors) html += `<div class="detail-item" style="padding-left:16px;color:#64748b">cors: ${{sec.cors}}</div>`;
        if (sec.rate_limit) html += `<div class="detail-item" style="padding-left:16px;color:#64748b">rate limit: ${{sec.rate_limit}}</div>`;
      }}

      // Input details
      if (ep.input) {{
        if (ep.input.body) {{
          html += `<div class="detail-item" style="padding-left:16px;color:#C9C9EB">body: ${{ep.input.body.name}}</div>`;
          if (ep.input.body.fields) {{
            Object.entries(ep.input.body.fields).forEach(([k, v]) => {{
              html += `<div class="detail-item" style="padding-left:28px;color:#475569">${{k}}: ${{v}}</div>`;
            }});
          }}
        }}
        if (ep.input.path_params?.length) {{
          ep.input.path_params.forEach(p => {{
            html += `<div class="detail-item" style="padding-left:16px;color:#FFA161">path: ${{p.name}} (${{p.param_type || p.type || ''}})</div>`;
          }});
        }}
        if (ep.input.query_params?.length) {{
          ep.input.query_params.forEach(p => {{
            html += `<div class="detail-item" style="padding-left:16px;color:#FFA161">query: ${{p.name}}${{p.required ? '' : ' (optional)'}}</div>`;
          }});
        }}
      }}

      // Validation rules
      if (ep.validation?.length) {{
        ep.validation.forEach(v => {{
          html += `<div class="detail-item" style="padding-left:16px;color:#E185C8">${{v.field}}: ${{v.constraints?.join(', ') || ''}}</div>`;
        }});
      }}

      // Behaviors (errors + success side effects)
      if (ep.behaviors) {{
        ep.behaviors.forEach(b => {{
          if (b.name !== 'success') {{
            html += `<div class="detail-item" style="padding-left:16px;color:#DF0F51">${{b.name}} &rarr; ${{b.returns?.status}}</div>`;
          }}
          if (b.side_effects?.length) {{
            b.side_effects.forEach(se => {{
              const desc = se.table || se.name || se.description || se.target || '';
              html += `<div class="detail-item" style="padding-left:16px;color:#34d399">&rarr; ${{se.kind || Object.keys(se)[0]}}: ${{desc}}</div>`;
            }});
          }}
        }});
      }}
    }});
    html += '</div>';

    // Flow diagrams for endpoints
    const endpointFlows = (spec.flows || []).filter(f => {{
      return cap.endpoints.some(ep => f.trigger.includes(ep.path));
    }});

    if (endpointFlows.length > 0) {{
      html += `<div class="flow-header"><h4>Request Flows</h4></div>`;
      endpointFlows.forEach((flow, idx) => {{
        const flowIdx = spec.flows.indexOf(flow);
        html += `<div class="detail-item" style="display:flex;justify-content:space-between;align-items:center">`;
        html += `<span>${{flow.trigger}}</span>`;
        html += `<button class="copy-btn" onclick="showFlowOverlay(${{flowIdx}})">View Flow ▶</button>`;
        html += `</div>`;
      }});
    }}
  }}

  if (cap.entities?.length) {{
    html += `<div class="detail-section"><h4>Entities (${{cap.entities.length}})</h4>`;
    cap.entities.forEach(ent => {{
      html += `<div class="detail-item" style="font-weight:600">${{ent.name}} <span style="color:#475569">table: ${{ent.table}}</span></div>`;
      (ent.fields || []).forEach(f => {{
        const type = f.type || f.field_type || '';
        html += `<div class="detail-item" style="padding-left:12px"><span style="color:#e2e8f0">${{f.name}}</span> <span style="color:#64748b">${{type}}</span></div>`;
        if (f.constraints?.length) {{
          f.constraints.forEach(c => {{
            html += `<div class="detail-item" style="padding-left:24px;color:#E185C8;font-size:10px">${{c}}</div>`;
          }});
        }}
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

// Graph — Two-level view: Domain bubbles (macro) → Node detail (micro)
const allDeps = spec.dependencies || [];
const svg = d3.select('#svg');
const graphEl = document.getElementById('graph');
const width = graphEl.clientWidth;
const height = graphEl.clientHeight;
svg.attr('viewBox', [0, 0, width, height]);

const tooltip = document.getElementById('tooltip');
const filterEl = document.getElementById('domain-filter');

// Build domain lookup
const domainOf = {{}};
(spec.domains || []).forEach(d => d.capabilities.forEach(c => domainOf[c] = d.name));
const domainNames = [...new Set(Object.values(domainOf))].sort();

const g = svg.append('g');
const zoomBehavior = d3.zoom().scaleExtent([0.1, 8]).on('zoom', (e) => g.attr('transform', e.transform));
svg.call(zoomBehavior);

g.append('defs').append('marker')
  .attr('id', 'arrow').attr('viewBox', '0 -5 10 10')
  .attr('refX', 28).attr('refY', 0).attr('markerWidth', 6).attr('markerHeight', 6)
  .attr('orient', 'auto')
  .append('path').attr('d', 'M0,-5L10,0L0,5').attr('fill', '#475569');

const USE_MACRO = spec.capabilities.length > 25;

function clearGraph() {{
  g.selectAll('*:not(defs)').remove();
  // Re-add marker after clear
  if (!g.select('#arrow').size()) {{
    g.append('defs').append('marker')
      .attr('id', 'arrow').attr('viewBox', '0 -5 10 10')
      .attr('refX', 28).attr('refY', 0).attr('markerWidth', 6).attr('markerHeight', 6)
      .attr('orient', 'auto')
      .append('path').attr('d', 'M0,-5L10,0L0,5').attr('fill', '#475569');
  }}
}}

// ===================== MACRO VIEW: Domain Bubbles =====================
function showMacroView() {{
  clearGraph();
  filterEl.innerHTML = '';

  // Build domain nodes
  const domainNodes = (spec.domains || []).map(d => {{
    const caps = d.capabilities.map(c => spec.capabilities.find(x => x.name === c)).filter(Boolean);
    const eps = caps.reduce((s, c) => s + (c.endpoints?.length || 0), 0);
    const ops = caps.reduce((s, c) => s + (c.operations?.length || 0), 0);
    const ents = caps.reduce((s, c) => s + (c.entities?.length || 0), 0);
    const size = Math.min(50, Math.max(20, 15 + d.capabilities.length * 3));
    return {{ id: d.name, count: d.capabilities.length, eps, ops, ents, size, extDeps: d.external_dependencies || [] }};
  }});

  // Build domain-level edges
  const domainEdges = [];
  const edgeSet = new Set();
  (spec.domains || []).forEach(d => {{
    (d.external_dependencies || []).forEach(ext => {{
      const key = d.name + '>' + ext;
      if (!edgeSet.has(key)) {{
        edgeSet.add(key);
        domainEdges.push({{ source: d.name, target: ext }});
      }}
    }});
  }});

  const sim = d3.forceSimulation(domainNodes)
    .force('link', d3.forceLink(domainEdges).id(d => d.id).distance(150))
    .force('charge', d3.forceManyBody().strength(-400))
    .force('center', d3.forceCenter(width / 2, height / 2))
    .force('collision', d3.forceCollide().radius(d => d.size + 20));

  const link = g.append('g').selectAll('line').data(domainEdges).join('line')
    .attr('class', 'link calls');

  const node = g.append('g').selectAll('g').data(domainNodes).join('g')
    .attr('cursor', 'pointer')
    .call(d3.drag()
      .on('start', (e, d) => {{ if (!e.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
      .on('drag', (e, d) => {{ d.fx = e.x; d.fy = e.y; }})
      .on('end', (e, d) => {{ if (!e.active) sim.alphaTarget(0); d.fx = null; d.fy = null; }}));

  // Bubble
  node.append('circle')
    .attr('r', d => d.size)
    .attr('fill', '#1e293b')
    .attr('stroke', '#334155')
    .attr('stroke-width', 2);

  // Colored ring showing composition
  node.append('circle')
    .attr('r', d => d.size - 3)
    .attr('fill', 'none')
    .attr('stroke', d => {{
      if (d.eps > 0) return '#E185C8';
      if (d.ents > 0) return '#C9C9EB';
      return '#34d399';
    }})
    .attr('stroke-width', 3)
    .attr('opacity', 0.6);

  // Domain name
  node.append('text')
    .text(d => d.id)
    .attr('text-anchor', 'middle')
    .attr('y', -4)
    .attr('fill', '#e2e8f0')
    .attr('font-size', 11)
    .attr('font-weight', 600);

  // Count
  node.append('text')
    .text(d => d.count + ' cap')
    .attr('text-anchor', 'middle')
    .attr('y', 10)
    .attr('fill', '#64748b')
    .attr('font-size', 9);

  // Hover tooltip
  node.on('mouseenter', (event, d) => {{
    let tt = `<div class="tt-name">${{d.id}}</div>`;
    tt += `<div class="tt-type">${{d.count}} capabilities</div>`;
    if (d.eps) tt += `<div class="tt-stat">${{d.eps}} endpoints</div>`;
    if (d.ops) tt += `<div class="tt-stat">${{d.ops}} operations</div>`;
    if (d.ents) tt += `<div class="tt-stat">${{d.ents}} entities</div>`;
    if (d.extDeps.length) tt += `<div class="tt-stat">depends on: ${{d.extDeps.join(', ')}}</div>`;
    tt += `<div class="tt-file">Click to drill in</div>`;
    tooltip.innerHTML = tt;
    tooltip.style.display = 'block';
    tooltip.style.left = (event.clientX + 14) + 'px';
    tooltip.style.top = (event.clientY - 10) + 'px';
  }})
  .on('mousemove', (event) => {{
    tooltip.style.left = (event.clientX + 14) + 'px';
    tooltip.style.top = (event.clientY - 10) + 'px';
  }})
  .on('mouseleave', () => {{ tooltip.style.display = 'none'; }})
  .on('click', (event, d) => {{ showDomainDetail(d.id); }});

  sim.on('tick', () => {{
    link.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
    node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
  }});
}}

// ===================== MICRO VIEW: Single Domain =====================
function showDomainDetail(domainName) {{
  clearGraph();
  svg.transition().duration(300).call(zoomBehavior.transform, d3.zoomIdentity);

  // Back button in filter area
  filterEl.innerHTML = `<button class="filter-btn active" onclick="showMacroView()"> All Domains</button><span style="color:#94a3b8;font-size:12px;margin-left:8px;font-weight:600">${{domainName}}</span>`;

  const domain = (spec.domains || []).find(d => d.name === domainName);
  if (!domain) return;

  const capNames = new Set(domain.capabilities);
  const caps = spec.capabilities.filter(c => capNames.has(c.name));

  const nodes = caps.map(c => ({{ id: c.name, ...getCapType(c), size: getCapSize(c) }}));
  const links = allDeps
    .filter(d => capNames.has(d.from) && capNames.has(d.to))
    .map(d => ({{ source: d.from, target: d.to, kind: d.kind || 'calls' }}));

  // External deps as ghost nodes
  const externalNodes = [];
  const externalLinks = [];
  allDeps.forEach(d => {{
    if (capNames.has(d.from) && !capNames.has(d.to)) {{
      const ghostId = '→ ' + d.to;
      if (!externalNodes.find(n => n.id === ghostId)) {{
        externalNodes.push({{ id: ghostId, type: 'External', color: '#334155', size: 6 }});
      }}
      externalLinks.push({{ source: d.from, target: ghostId, kind: d.kind || 'calls' }});
    }}
    if (!capNames.has(d.from) && capNames.has(d.to)) {{
      const ghostId = d.from + ' →';
      if (!externalNodes.find(n => n.id === ghostId)) {{
        externalNodes.push({{ id: ghostId, type: 'External', color: '#334155', size: 6 }});
      }}
      externalLinks.push({{ source: ghostId, target: d.to, kind: d.kind || 'calls' }});
    }}
  }});

  const allNodes = [...nodes, ...externalNodes];
  const allLinks = [...links, ...externalLinks];

  const sim = d3.forceSimulation(allNodes)
    .force('link', d3.forceLink(allLinks).id(d => d.id).distance(100))
    .force('charge', d3.forceManyBody().strength(-300))
    .force('center', d3.forceCenter(width / 2, height / 2))
    .force('collision', d3.forceCollide().radius(d => d.size + 15));

  const link = g.append('g').selectAll('line').data(allLinks).join('line')
    .attr('class', d => 'link ' + d.kind);

  const node = g.append('g').selectAll('g').data(allNodes).join('g').attr('class', 'node')
    .attr('cursor', 'pointer')
    .call(d3.drag()
      .on('start', (e, d) => {{ if (!e.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
      .on('drag', (e, d) => {{ d.fx = e.x; d.fy = e.y; }})
      .on('end', (e, d) => {{ if (!e.active) sim.alphaTarget(0); d.fx = null; d.fy = null; }}));

  node.append('circle')
    .attr('r', d => d.size)
    .attr('fill', d => d.color)
    .attr('stroke', '#1e293b')
    .attr('stroke-width', 2);

  node.append('text').text(d => d.id).attr('x', d => d.size + 5).attr('y', 4)
    .attr('fill', d => d.type === 'External' ? '#475569' : '#94a3b8')
    .attr('font-size', 11)
    .attr('font-style', d => d.type === 'External' ? 'italic' : 'normal');

  node.on('mouseenter', (event, d) => {{
    const cap = spec.capabilities.find(c => c.name === d.id);
    if (!cap) return;
    const info = getCapType(cap);
    let tt = `<div class="tt-name">${{d.id}}</div><div class="tt-type">${{info.type}}</div>`;
    if (cap.endpoints?.length) tt += `<div class="tt-stat">${{cap.endpoints.length}} endpoints</div>`;
    if (cap.operations?.length) tt += `<div class="tt-stat">${{cap.operations.length}} operations</div>`;
    if (cap.entities?.length) tt += `<div class="tt-stat">${{cap.entities.length}} entities</div>`;
    if (cap.source) tt += `<div class="tt-file">${{cap.source}}</div>`;
    tooltip.innerHTML = tt;
    tooltip.style.display = 'block';
    tooltip.style.left = (event.clientX + 14) + 'px';
    tooltip.style.top = (event.clientY - 10) + 'px';

    const connected = new Set([d.id]);
    allLinks.forEach(l => {{
      const sId = typeof l.source === 'object' ? l.source.id : l.source;
      const tId = typeof l.target === 'object' ? l.target.id : l.target;
      if (sId === d.id) connected.add(tId);
      if (tId === d.id) connected.add(sId);
    }});
    node.classed('dimmed', n => !connected.has(n.id));
    link.classed('dimmed', l => {{
      const sId = typeof l.source === 'object' ? l.source.id : l.source;
      const tId = typeof l.target === 'object' ? l.target.id : l.target;
      return sId !== d.id && tId !== d.id;
    }});
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
    if (d.type !== 'External') showDetail(d.id);
  }});

  sim.on('tick', () => {{
    link.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
    node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
  }});
}}

// ===================== CLASSIC VIEW: Small Projects =====================
function showClassicView() {{
  clearGraph();
  const nodes = spec.capabilities.map(c => ({{ id: c.name, ...getCapType(c), size: getCapSize(c) }}));
  const links = allDeps.map(d => ({{ source: d.from, target: d.to, kind: d.kind || 'calls' }}));

  const sim = d3.forceSimulation(nodes)
    .force('link', d3.forceLink(links).id(d => d.id).distance(100))
    .force('charge', d3.forceManyBody().strength(-250))
    .force('center', d3.forceCenter(width / 2, height / 2))
    .force('collision', d3.forceCollide().radius(d => d.size + 15));

  const link = g.append('g').selectAll('line').data(links).join('line')
    .attr('class', d => 'link ' + (d.kind || 'calls'));

  const node = g.append('g').selectAll('g').data(nodes).join('g').attr('class', 'node')
    .attr('cursor', 'pointer')
    .call(d3.drag()
      .on('start', (e, d) => {{ if (!e.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
      .on('drag', (e, d) => {{ d.fx = e.x; d.fy = e.y; }})
      .on('end', (e, d) => {{ if (!e.active) sim.alphaTarget(0); d.fx = null; d.fy = null; }}));

  node.append('circle').attr('r', d => d.size).attr('fill', d => d.color)
    .attr('stroke', '#1e293b').attr('stroke-width', 2);

  node.append('text').text(d => d.id).attr('x', d => d.size + 5).attr('y', 4)
    .attr('fill', '#94a3b8').attr('font-size', 11);

  node.on('mouseenter', (event, d) => {{
    const cap = spec.capabilities.find(c => c.name === d.id);
    if (!cap) return;
    const info = getCapType(cap);
    let tt = `<div class="tt-name">${{d.id}}</div><div class="tt-type">${{info.type}}</div>`;
    if (cap.endpoints?.length) tt += `<div class="tt-stat">${{cap.endpoints.length}} endpoints</div>`;
    if (cap.operations?.length) tt += `<div class="tt-stat">${{cap.operations.length}} operations</div>`;
    if (cap.source) tt += `<div class="tt-file">${{cap.source}}</div>`;
    tooltip.innerHTML = tt;
    tooltip.style.display = 'block';
    tooltip.style.left = (event.clientX + 14) + 'px';
    tooltip.style.top = (event.clientY - 10) + 'px';
  }})
  .on('mousemove', (event) => {{ tooltip.style.left = (event.clientX + 14) + 'px'; tooltip.style.top = (event.clientY - 10) + 'px'; }})
  .on('mouseleave', () => {{ tooltip.style.display = 'none'; }})
  .on('click', (event, d) => {{ showDetail(d.id); }});

  sim.on('tick', () => {{
    link.attr('x1', d => d.source.x).attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
    node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
  }});
}}

// Start the appropriate view
if (USE_MACRO) {{
  showMacroView();
}} else if (allDeps.length > 0) {{
  showClassicView();
}} else {{
  svg.append('text').attr('x', width/2).attr('y', height/2).attr('text-anchor', 'middle')
    .attr('fill', '#475569').attr('font-size', 14).text('No dependencies to visualize');
}}
</script>
</body>
</html>"##
    )
}
