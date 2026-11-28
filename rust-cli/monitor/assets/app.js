(function () {
  'use strict';
  // The token lives only in this closure. It is read once, then the address is reset to '/'.
  const hashMatch = /^#token=([0-9a-f]{32})$/.exec(location.hash);
  const TOKEN = hashMatch ? hashMatch[1] : null;
  history.replaceState(null, '', '/');

  const $ = (id) => document.getElementById(id);
  const pad = (n, w) => String(n).padStart(w || 2, '0');
  const hms = (d) => (isNaN(d) ? '--:--:--' : pad(d.getHours()) + ':' + pad(d.getMinutes()) + ':' + pad(d.getSeconds()));
  function tz() {
    const o = -new Date().getTimezoneOffset();
    const a = Math.abs(o);
    return 'UTC' + (o < 0 ? '-' : '+') + pad(Math.floor(a / 60)) + ':' + pad(a % 60);
  }
  function el(tag, cls, text) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }

  const POLL_MS = 5000;
  const STALE_MS = 15000;
  const QUIET_MS = 15 * 60 * 1000;
  const GLYPH = { warn: '⚠︎', info: 'ℹ︎', bad: '✕' };
  const OUTCOMES = {
    ok: ['✓', 'OK', 'o-ok'],
    blocked: ['⊘', 'Blocked', 'o-blocked'],
    error: ['✕', 'Error', 'o-error'],
    started: ['○', 'Started', 'o-null']
  };
  const KINDS = ['mcp.request.started', 'mcp.tool.completed', 'mcp.protocol.error', 'operation.started',
    'operation.completed', 'workflow.stage_selected', 'workflow.stage_reported', 'workflow.gate_decided'];
  const ERROR_COPY = {
    invalid_arguments: 'Arguments were rejected', approver_required: 'An approver name was required',
    state_conflict: 'Workflow state conflict', operation_failed: 'Operation failed',
    unknown_tool: 'Unknown tool requested', internal_error: 'Internal error',
    line_too_long: 'Request line too long', parse_error: 'Request was not valid JSON',
    invalid_request: 'Invalid request', method_not_found: 'Method not found',
    invalid_params: 'Invalid parameters', unsupported_protocol_version: 'Unsupported protocol version',
    protocol_error: 'Protocol error'
  };
  const HTTP_COPY = {
    busy: 'the monitor is busy', internal_error: 'monitor internal error',
    invalid_filter: 'a filter value was rejected', range_too_large: 'range too large',
    request_timeout: 'request timed out'
  };
  const NOT_RETRIED = ['invalid_filter', 'range_too_large', 'request_timeout', 'bad_request', 'header_too_large',
    'method_not_allowed', 'host_rejected', 'origin_rejected', 'not_found'];
  const RULES = {
    stage: [(v) => /^[a-z0-9-]{1,64}$/.test(v), "Use lowercase letters, digits or '-', up to 64 characters"],
    session: [(v) => /^[0-9a-f]{32}$/.test(v), 'Use the 32 lowercase hex characters of a session id'],
    trace: [(v) => /^[0-9a-f]{32}-[0-9]+$/.test(v) && v.length <= 64, "Use a session id, '-', then a number, up to 64 characters"]
  };
  const FILTER_KEYS = ['component', 'name', 'outcome', 'stage', 'session', 'trace'];
  const emptyFilters = () => ({ component: '', name: '', outcome: '', stage: '', session: '', trace: '' });

  // Everything the page knows lives in this one object (memory only).
  const st = {
    mode: 'init', // init ok error unavailable failed stopped unexpected tokenMissing tokenRejected
    data: null, events: [], range: 3600, applied: emptyFilters(),
    paused: false, lastOk: 0, retryAt: 0, backoff: 5, failCode: '', notice: '',
    selected: null, detail: null, parentMsg: ''
  };
  let seq = 0;
  let ctrl = null;
  let timer = null;
  let pending = [];
  const rows = new Map();

  function say(t) { pending.push(t); }
  function flush() {
    if (pending.length) $('status').textContent = pending.join(' ');
    pending = [];
  }

  // ---- banners: keyed nodes, updated in place so focus and countdowns survive ----
  function buildBanner(s) {
    const n = el('div', 'banner' + (s.cls ? ' ' + s.cls : ''));
    n.dataset.key = s.key;
    if (s.glyph) {
      const g = el('span', 'g', s.glyph);
      g.setAttribute('aria-hidden', 'true');
      n.appendChild(g);
    }
    n.appendChild(el('strong', 'h', s.head));
    s.lines.forEach((l) => n.appendChild(el('p', 'l', l)));
    (s.buttons || []).forEach((b) => {
      const btn = el('button', null, b[0]);
      btn.type = 'button';
      btn.addEventListener('click', b[1]);
      n.appendChild(btn);
    });
    return n;
  }
  function paint(box, specs) {
    const want = new Set(specs.map((s) => s.key));
    Array.from(box.children).forEach((c) => { if (!want.has(c.dataset.key)) c.remove(); });
    specs.forEach((s, i) => {
      let n = Array.from(box.children).find((c) => c.dataset.key === s.key);
      if (n) {
        n.querySelector('.h').textContent = s.head;
        n.querySelectorAll('.l').forEach((p, j) => { p.textContent = s.lines[j]; });
      } else {
        n = buildBanner(s);
        if (!s.silent) say([s.head].concat(s.lines).join(' '));
      }
      if (box.children[i] !== n) box.insertBefore(n, box.children[i] || null);
    });
    if (box.id !== 'notes') box.hidden = specs.length === 0;
  }

  const rangeLabel = () => ({ 900: '15 minutes', 3600: '1 hour', 86400: '24 hours', 604800: '7 days' }[st.range] || 'range');
  const staleLine = () => (st.data ? 'Last data from ' + hms(new Date(st.lastOk)) + ' shown, marked STALE.' : 'No data loaded yet.');
  const retryIn = () => 'Retrying in ' + Math.max(0, Math.ceil((st.retryAt - Date.now()) / 1000)) + ' s.';
  const hasActiveFilter = () => FILTER_KEYS.some((k) => st.applied[k]);

  function bannerSpec() {
    const m = st.mode;
    const reopen = ['Reopen the address printed in the terminal where you ran dtech-aidlc monitor.',
      'Reloading this page removes the token on purpose.'];
    if (m === 'tokenMissing') return { key: 'tokenMissing', cls: 'bad', glyph: GLYPH.warn, head: 'Token missing', lines: reopen };
    if (m === 'tokenRejected') return { key: 'tokenRejected', cls: 'bad', glyph: GLYPH.warn, head: 'Token rejected', lines: reopen };
    if (m === 'stopped' || m === 'unexpected') {
      return {
        key: m, cls: 'bad', glyph: GLYPH.bad,
        head: m === 'stopped' ? 'Monitor stopped or unreachable.' : 'Unexpected response from the monitor address.',
        lines: ['Auto-refresh has stopped.', staleLine()],
        buttons: [['Retry once', () => run('retry')]]
      };
    }
    if (m === 'unavailable') {
      return {
        key: 'unavailable', cls: 'warn', glyph: GLYPH.warn, head: 'Telemetry store unavailable',
        lines: ['The .aidlc/telemetry folder is unsafe or unreadable now.', staleLine() + ' ' + retryIn()],
        buttons: [['Retry now', () => run('retry')]]
      };
    }
    if (m === 'error') {
      return {
        key: 'error:' + st.failCode, cls: 'warn', glyph: GLYPH.warn,
        head: 'Request failed: ' + (HTTP_COPY[st.failCode] || 'monitor error') + '.',
        lines: [retryIn(), staleLine()], buttons: [['Retry now', () => run('retry')]]
      };
    }
    if (m === 'failed') {
      return {
        key: 'failed:' + st.failCode, cls: 'warn', glyph: GLYPH.warn,
        head: 'Request failed: ' + (HTTP_COPY[st.failCode] || 'bad request') + '.',
        lines: ['Change the filters or press Refresh.', staleLine()]
      };
    }
    if (st.paused) {
      return {
        key: 'paused', glyph: GLYPH.info, head: 'Auto-refresh paused while older events are shown.', lines: [],
        buttons: [['Back to newest', () => run('newest')]]
      };
    }
    if (m === 'ok' && st.lastOk && Date.now() - st.lastOk > STALE_MS) {
      const age = Math.round((Date.now() - st.lastOk) / 1000);
      return { key: 'stale', cls: 'warn', glyph: GLYPH.warn, head: 'STALE: last updated ' + hms(new Date(st.lastOk)) + ' (' + age + ' s ago).', lines: ['Refreshing...'] };
    }
    if (m === 'init') return { key: 'loading', head: 'Loading telemetry...', lines: [], silent: true };
    return null;
  }

  function notesSpecs() {
    const out = [];
    const d = st.data;
    if (!d || st.mode === 'tokenRejected') return out;
    if (d.limitReached) {
      out.push({
        key: 'limit', cls: 'warn', glyph: GLYPH.warn, head: 'Partial result: a read limit was reached (512 files, 64 MiB or 250,000 events).',
        lines: ['Newest data shown; summary is partial.', 'Narrow the range or filters.']
      });
    }
    const last = d.summary.lastEventAt;
    if (last && Date.parse(d.to) - Date.parse(last) > QUIET_MS) {
      out.push({ key: 'quiet', glyph: GLYPH.info, head: 'Quiet: no event in the last 15 min (last ' + hms(new Date(last)) + ').', lines: ['Not a liveness claim.'] });
    }
    if (st.notice === 'stale_cursor') {
      out.push({ key: 'cursor', glyph: GLYPH.info, head: 'Results changed since this page was loaded.', lines: ['Showing the newest events again.'] });
    }
    return out;
  }

  function emptySpec() {
    const d = st.data;
    if (!d || st.events.length || d.summary.lastEventAt !== null) return [];
    if (hasActiveFilter()) {
      return [{
        key: 'emptyf', glyph: GLYPH.info, head: 'No events match these filters in the last ' + rangeLabel() + '.', lines: [],
        buttons: [['Clear filters', () => clearFilters()]]
      }];
    }
    return [{
      key: 'empty', glyph: GLYPH.info, head: 'No telemetry observed in the last ' + rangeLabel() + '.',
      lines: ['This does not mean the MCP process is absent or that telemetry is off.']
    }];
  }

  // ---- summary, coverage, list, detail ----
  function setSummary(d) {
    const s = d.summary;
    const part = d.limitReached ? ' (partial)' : '';
    $('s-tool').textContent = s.toolCalls + part;
    $('s-blocked').textContent = s.blocked.count + ' of ' + s.blocked.of + part;
    $('s-errors').textContent = s.errors.count + ' of ' + s.errors.of + part;
    $('s-incomplete').textContent = s.incomplete + ' (never counted as ok)' + part;
    $('s-ops').textContent = s.operations.count + ' (errors ' + s.operations.errors + '), separate population' + part;
    $('s-gate').textContent = s.eventKinds['workflow.gate_decided'] + ' (telemetry, not audit; no approve/reject value)' + part;
    const l = s.latency;
    $('s-latency').textContent = l.insufficient
      ? 'insufficient samples (n = ' + l.n + ', needs 20)' + part
      : 'p50 ' + l.p50 + ' ms · p95 ' + l.p95 + ' ms · n = ' + l.n + part;
    $('s-last').textContent = s.lastEventAt ? hms(new Date(s.lastEventAt)) : 'none';

    const c = d.coverage;
    const cov = $('coverage');
    cov.textContent = '';
    const bits = [];
    const mark = (n) => {
      const sp = el('span', null, n > 0 ? '⚠︎ ' : '');
      sp.setAttribute('aria-hidden', 'true');
      return sp;
    };
    bits.push(['Coverage: files read ' + c.filesRead]);
    bits.push([mark(c.skippedLines), 'skipped lines ' + c.skippedLines + (c.skippedLines > 0 ? ' (malformed or unknown records not shown)' : '')]);
    bits.push([mark(c.partialTails), 'partial tails ' + c.partialTails + (c.partialTails > 0 ? ' (unterminated last line not shown)' : '')]);
    bits.push(['unreadable files ' + c.unreadableFiles]);
    bits.push(['expired files ' + c.expiredFiles]);
    bits.push(['Dropped events: not recorded']);
    bits.push(['CLI writes no events']);
    if (!hasActiveFilter()) {
      const none = KINDS.filter((k) => s.eventKinds[k] === 0);
      bits.push(['kinds not observed: ' + (none.length ? none.join(', ') : 'none') + part]);
    }
    bits.forEach((b, i) => {
      if (i) cov.appendChild(document.createTextNode(' · '));
      b.forEach((x) => cov.appendChild(typeof x === 'string' ? document.createTextNode(x) : x));
    });

    const to = new Date(d.to);
    const from = new Date(to.getTime() - d.range * 1000);
    $('range-label').textContent = hms(from) + ' to ' + hms(to) + ' (' + tz() + ')';
  }

  function outcomeNode(o) {
    const [g, w, cls] = OUTCOMES[o || 'started'];
    const s = el('span', cls);
    const gl = el('span', null, g);
    gl.setAttribute('aria-hidden', 'true');
    s.appendChild(gl);
    s.appendChild(document.createTextNode(' ' + w));
    return s;
  }
  const outWord = (o) => OUTCOMES[o || 'started'][1];
  const srcLabel = (e) => (e.component === 'mcp' ? 'MCP' : 'APP');
  const msText = (e) => (e.durationMs == null ? '--' : e.durationMs + ' ms');
  const shortTrace = (id) => (/^[0-9a-f]{32}-[0-9]+$/.test(id) ? id.slice(0, 8) + '..' + id.slice(32) : id.slice(0, 12));

  function makeRow(e) {
    const li = el('li');
    const b = el('button', 'row-btn');
    b.type = 'button';
    const t = hms(new Date(e.timestamp));
    b.setAttribute('aria-label', [t, srcLabel(e), e.event, e.name, e.durationMs == null ? 'no duration' : e.durationMs + ' ms', outWord(e.outcome)].join(', '));
    b.appendChild(el('span', 'c-time', t));
    b.appendChild(el('span', 'c-src', srcLabel(e)));
    b.appendChild(el('span', 'c-ev', e.event));
    b.appendChild(el('span', 'c-name', e.name));
    b.appendChild(el('span', 'c-ms', msText(e)));
    const out = outcomeNode(e.outcome);
    out.className += ' c-out';
    b.appendChild(out);
    b.appendChild(el('span', 'c-trace', shortTrace(e.traceId)));
    b.addEventListener('click', () => select(e));
    li.appendChild(b);
    return li;
  }

  function renderList() {
    const ol = $('list');
    const ids = new Set(st.events.map((e) => e.eventId));
    rows.forEach((li, id) => {
      if (ids.has(id)) return;
      const had = li.contains(document.activeElement);
      li.remove();
      rows.delete(id);
      if (had) $('events-h').focus();
    });
    let ref = ol.firstChild;
    st.events.forEach((e) => {
      let li = rows.get(e.eventId);
      if (!li) { li = makeRow(e); rows.set(e.eventId, li); }
      if (li === ref) ref = ref.nextSibling;
      else ol.insertBefore(li, ref);
    });
    rows.forEach((li, id) => {
      const b = li.firstChild;
      if (id === st.selected) b.setAttribute('aria-current', 'true');
      else b.removeAttribute('aria-current');
    });
    const d = st.data;
    $('end').hidden = !d || d.nextCursor !== null || st.events.length === 0;
    $('older').hidden = !d || d.nextCursor === null;
    const act = activeText();
    $('events-meta').textContent = 'newest first · ' + st.events.length + ' shown' + (act ? ' · ' + act : '');
  }

  function activeText() {
    const a = st.applied;
    const parts = [];
    ['component', 'name', 'outcome', 'stage'].forEach((k) => { if (a[k]) parts.push(k + ' ' + a[k]); });
    if (a.session) parts.push('session ' + a.session.slice(0, 8) + '..');
    if (a.trace) parts.push('trace ' + shortTrace(a.trace));
    return parts.join(' · ');
  }
  function renderActive() {
    const n = FILTER_KEYS.filter((k) => st.applied[k]).length;
    $('filters-h').textContent = 'Filters (' + n + ')';
    const t = activeText();
    $('active').textContent = t ? 'Active: ' + t : '';
  }

  function addKv(dl, k, v, id) {
    dl.appendChild(el('dt', null, k));
    dl.appendChild(el('dd', id ? 'id' : null, v));
  }
  function renderDetail() {
    const e = st.detail;
    $('detail').hidden = !e;
    if (!e) return;
    const d = new Date(e.timestamp);
    $('d-title').textContent = e.event + ' · ' + e.name;
    const o = $('d-outcome');
    o.textContent = '';
    o.appendChild(outcomeNode(e.outcome));
    const dl = $('d-kv');
    dl.textContent = '';
    addKv(dl, 'Time', hms(d) + '.' + pad(d.getMilliseconds(), 3) + ' (' + tz() + ')');
    addKv(dl, 'UTC', e.timestamp, true);
    addKv(dl, 'Source', e.component);
    addKv(dl, 'Caller', e.caller);
    addKv(dl, 'Duration', e.durationMs == null ? 'none' : e.durationMs + ' ms');
    let err = 'none' + (e.outcome ? ' (outcome ' + e.outcome + ')' : '');
    if (e.errorCode) err = e.errorCode + ': ' + (ERROR_COPY[e.errorCode] || 'unrecognised code');
    addKv(dl, 'Error', err, !!e.errorCode);
    addKv(dl, 'Intent', e.intentId || 'not recorded', !!e.intentId);
    addKv(dl, 'Stage', e.stageId || 'not recorded', !!e.stageId);
    addKv(dl, 'Unit', e.unitId || 'not recorded', !!e.unitId);
    addKv(dl, 'Event id', e.eventId, true);
    addKv(dl, 'Session', e.sessionId, true);
    addKv(dl, 'Trace', e.traceId, true);
    addKv(dl, 'Span', e.spanId, true);
    addKv(dl, 'Parent', st.parentMsg || (e.parentSpanId === null ? 'no parent (root span)' : e.parentSpanId), e.parentSpanId !== null && !st.parentMsg);
    addKv(dl, 'Process', String(e.processId));
    $('d-parent').hidden = e.parentSpanId === null || !!st.parentMsg;
  }
  function select(e) {
    st.detail = e;
    st.selected = e.eventId;
    st.parentMsg = '';
    renderDetail();
    renderList();
    $('detail-h').focus();
  }
  function closeDetail() {
    const had = st.selected;
    st.detail = null;
    st.selected = null;
    renderDetail();
    renderList();
    const li = rows.get(had);
    if (li) li.firstChild.focus();
    else $('events-h').focus();
  }

  async function parentLookup() {
    const e = st.detail;
    if (!e || e.parentSpanId === null) return;
    const pick = (list) => {
      const m = list.filter((x) => x.spanId === e.parentSpanId);
      return m.find((x) => /\.completed$/.test(x.event)) || m[0] || null;
    };
    let found = pick(st.events);
    if (!found) {
      const p = new URLSearchParams();
      p.set('range', String(st.range));
      p.set('limit', '200');
      p.set('trace', e.traceId);
      const r = await fetchJson(p.toString(), new AbortController().signal);
      if (st.detail !== e) return;
      if (r.t === 'ok') found = pick(r.body.events);
      else st.parentMsg = 'parent lookup failed';
    }
    if (found) { st.detail = found; st.selected = found.eventId; st.parentMsg = ''; }
    else if (!st.parentMsg) st.parentMsg = 'parent not in store (expired, skipped, dropped or outside the range)';
    renderDetail();
    renderList();
    $('detail-h').focus();
  }

  // ---- requests ----
  function valid(b) {
    return b && b.schemaVersion === 1 && Array.isArray(b.events) && typeof b.to === 'string' && typeof b.range === 'number' &&
      b.summary && typeof b.summary.toolCalls === 'number' && b.summary.blocked && b.summary.errors &&
      b.summary.operations && b.summary.latency && b.summary.eventKinds && b.coverage;
  }
  async function fetchJson(query, signal) {
    let res;
    try {
      res = await fetch('/api/events?' + query, {
        method: 'GET', headers: { Authorization: 'Bearer ' + TOKEN }, cache: 'no-store',
        credentials: 'omit', redirect: 'error', referrerPolicy: 'no-referrer', signal: signal
      });
    } catch (e) {
      return { t: e && e.name === 'AbortError' ? 'abort' : 'network' };
    }
    if ((res.headers.get('content-type') || '').indexOf('application/json') !== 0) return { t: 'unexpected' };
    let body;
    try { body = await res.json(); } catch (e) { return { t: e && e.name === 'AbortError' ? 'abort' : 'unexpected' }; }
    if (res.status === 200) return valid(body) ? { t: 'ok', body: body } : { t: 'unexpected' };
    return { t: 'error', status: res.status, code: body && typeof body.error === 'string' ? body.error : '' };
  }
  function query(cursor) {
    const p = new URLSearchParams();
    p.set('range', String(st.range));
    p.set('limit', '50');
    FILTER_KEYS.forEach((k) => { if (st.applied[k]) p.set(k, st.applied[k]); });
    if (cursor) p.set('cursor', cursor);
    return p.toString();
  }

  function schedule(ms) {
    clearTimeout(timer);
    timer = null;
    if (st.paused || !TOKEN) return;
    timer = setTimeout(() => { timer = null; if (!document.hidden) run('auto'); }, ms);
  }
  function clearList() {
    rows.forEach((li) => li.remove());
    rows.clear();
  }

  // kind: auto | refresh | apply | clear | range | older | newest | retry. Only 'auto' is silent.
  async function run(kind, extra) {
    if (!TOKEN) return;
    extra = extra || {};
    clearTimeout(timer);
    timer = null;
    if (ctrl) ctrl.abort();
    ctrl = new AbortController();
    const my = ++seq;
    const manual = kind !== 'auto';
    if (manual && !extra.keepNotice) st.notice = '';
    if (kind === 'retry') st.retryAt = 0;
    const older = kind === 'older' && st.data && st.data.nextCursor;
    if (!st.data) $('main').setAttribute('aria-busy', 'true');
    const r = await fetchJson(query(older ? st.data.nextCursor : null), ctrl.signal);
    if (my !== seq) return; // a newer request owns the page
    $('main').setAttribute('aria-busy', 'false');
    if (r.t === 'network' || r.t === 'unexpected') {
      st.mode = r.t === 'network' ? 'stopped' : 'unexpected';
    } else if (r.t === 'ok') {
      const b = r.body;
      if (older) {
        const seen = new Set(st.events.map((e) => e.eventId));
        st.events = st.events.concat(b.events.filter((e) => !seen.has(e.eventId)));
        st.paused = true;
      } else {
        st.events = b.events;
        st.paused = false;
      }
      st.events = st.events.filter((e) => e.schemaVersion === 1);
      st.data = b;
      st.mode = 'ok';
      st.backoff = 5;
      st.lastOk = Date.now();
      if (extra.notice) st.notice = extra.notice;
      if (manual) say(extra.msg ? extra.msg(b) : 'Updated ' + hms(new Date()) + ', ' + st.events.length + ' events');
      schedule(POLL_MS);
      render();
      if (older && b.events[0] && rows.get(b.events[0].eventId)) rows.get(b.events[0].eventId).firstChild.focus();
      else if (extra.focus === 'events') $('events-h').focus();
      return;
    } else {
      const { status, code } = r;
      if (status === 401 && code === 'unauthorized') {
        st.mode = 'tokenRejected';
        st.data = null;
        st.events = [];
        st.detail = null;
        st.selected = null;
        clearList();
      } else if (status === 409 && code === 'cursor_stale') {
        return run('newest', { notice: 'stale_cursor', keepNotice: true, focus: 'events' });
      } else if ((status === 500 && code === 'internal_error') || (status === 503 && (code === 'busy' || code === 'store_unavailable'))) {
        st.mode = code === 'store_unavailable' ? 'unavailable' : 'error';
        st.failCode = code;
        st.paused = false; // retries fetch the newest page, so Load older's pause no longer applies
        st.retryAt = Date.now() + st.backoff * 1000;
        schedule(st.backoff * 1000);
        st.backoff = Math.min(60, st.backoff * 2);
      } else if (status >= 400 && status < 500 && NOT_RETRIED.indexOf(code) !== -1) {
        st.mode = 'failed';
        st.failCode = code;
        if (kind === 'apply') $('f-component').focus();
      } else {
        st.mode = 'unexpected';
      }
    }
    render();
  }

  // ---- filters ----
  function setErr(k, msg) {
    const input = $('f-' + k);
    let p = $('f-' + k + '-err');
    if (!msg) {
      if (p) p.remove();
      input.removeAttribute('aria-invalid');
      if ($('f-' + k + '-hint')) input.setAttribute('aria-describedby', 'f-' + k + '-hint');
      else input.removeAttribute('aria-describedby');
      return;
    }
    if (!p) {
      p = el('p', 'err');
      p.id = 'f-' + k + '-err';
      input.parentNode.appendChild(p);
    }
    p.textContent = '';
    const g = el('span', null, GLYPH.bad + ' ');
    g.setAttribute('aria-hidden', 'true');
    p.appendChild(g);
    p.appendChild(document.createTextNode(msg));
    input.setAttribute('aria-invalid', 'true');
    input.setAttribute('aria-describedby', 'f-' + k + '-err');
  }
  function syncForm() {
    FILTER_KEYS.forEach((k) => { $('f-' + k).value = st.applied[k]; setErr(k, ''); });
  }
  function clearFilters() {
    st.applied = emptyFilters();
    syncForm();
    run('clear');
  }
  function onApply(ev) {
    ev.preventDefault();
    const next = emptyFilters();
    let bad = null;
    FILTER_KEYS.forEach((k) => {
      const v = $('f-' + k).value.trim();
      next[k] = v;
      const rule = RULES[k];
      const ok = !v || !rule || rule[0](v);
      setErr(k, ok ? '' : rule[1]);
      if (!ok && !bad) bad = k;
    });
    if (bad) { $('f-' + bad).focus(); return; }
    st.applied = next;
    run('apply');
  }
  function showOnly(k, v) {
    st.applied[k === 'trace' ? 'session' : 'trace'] = '';
    st.applied[k] = v;
    syncForm();
    run('apply', { focus: 'events', msg: (b) => 'Filtered to ' + k + ' ' + shortTrace(v) + ', ' + b.events.length + ' events' });
  }

  // ---- render ----
  function renderBanners() {
    paint($('banner'), [bannerSpec()].filter(Boolean));
    paint($('notes'), notesSpecs());
    paint($('empty'), st.data && st.mode !== 'tokenRejected' ? emptySpec() : []);
    flush();
  }
  function render() {
    const m = st.mode;
    const noToken = m === 'tokenMissing' || m === 'tokenRejected';
    const has = !!st.data && !noToken;
    const bannerHad = $('banner').contains(document.activeElement);
    $('range-band').hidden = noToken;
    $('filters').hidden = noToken;
    $('summary').hidden = !has;
    $('cols').hidden = !has;
    $('skip').hidden = !has;
    if (has) {
      setSummary(st.data);
      renderList();
      $('updated').textContent = 'Status: Updated ' + hms(new Date(st.lastOk)) + ' · ' +
        (st.paused ? 'auto-refresh paused' : 'auto-refresh every 5 s');
    }
    renderActive();
    renderBanners();
    if (bannerHad && (document.activeElement === document.body || !document.activeElement)) {
      if (has) $('events-h').focus();
    }
    flush();
  }

  // ---- wiring ----
  $('filter-form').addEventListener('submit', onApply);
  $('clear').addEventListener('click', clearFilters);
  $('refresh').addEventListener('click', () => run('refresh'));
  $('range').addEventListener('change', () => { st.range = Number($('range').value); run('range'); });
  $('older').addEventListener('click', () => run('older'));
  $('d-close').addEventListener('click', closeDetail);
  $('d-trace').addEventListener('click', () => st.detail && showOnly('trace', st.detail.traceId));
  $('d-session').addEventListener('click', () => st.detail && showOnly('session', st.detail.sessionId));
  $('d-parent').addEventListener('click', parentLookup);
  $('detail').addEventListener('keydown', (e) => { if (e.key === 'Escape') closeDetail(); });
  $('skip').addEventListener('click', (e) => { e.preventDefault(); $('events-h').focus(); });

  const wide = window.matchMedia('(min-width: 60rem)');
  if (!wide.matches) $('filters-box').open = false;
  wide.addEventListener('change', () => { if (wide.matches) $('filters-box').open = true; });

  document.addEventListener('visibilitychange', () => {
    if (document.hidden || !TOKEN || st.paused) return;
    if (st.mode === 'ok' || st.mode === 'error' || st.mode === 'unavailable') run('auto');
  });
  setInterval(() => { if (TOKEN) renderBanners(); }, 1000);

  if (!TOKEN) {
    st.mode = 'tokenMissing';
    // Insert after load so the live region announces it.
    setTimeout(render, 100);
  } else {
    render();
    run('auto');
  }
})();
