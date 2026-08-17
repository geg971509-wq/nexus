  const powerBtn = document.getElementById('powerBtn');
  const statusDot = document.getElementById('statusDot');
  const statusText = document.getElementById('statusText');
  const statusSub = document.getElementById('statusSub');
  const sbStatus = document.getElementById('sbStatus');
  const sbProxy = document.getElementById('sbProxy');
  const logPanel = document.getElementById('logPanel');
  const nodeTable = document.getElementById('nodeTable');
  const tableCard = document.querySelector('.table-card');
  let nodeListPointerInside = false;
  let nodeListFocus = false;
  if (tableCard) {
    tableCard.tabIndex = 0; // focus target so ⌘A works after click in list
    tableCard.setAttribute('role', 'grid');
    tableCard.setAttribute('aria-label', '节点列表');
  }
  let connected = false;
  let powerBusy = false;
  let selectedName = '—';
  let selectedLat = '—';
  // Hero "经 xxx" follows the tunnel actually started — not the list selection.
  let connectedName = '';
  let connectedLat = '—';
  // default first open: 延迟升序（快→慢；未知/超时仍沉底）
  let sortKey = 'lat';
  let sortDir = 1; // 1 asc, -1 desc

  // AutoSelector: missing/timeout latency always parks at the end in BOTH directions.
  // Returns { known, ms } — only known positive (or 0) ms participate in numeric order.
  function parseLatKey(row) {
    const raw = (row.dataset.lat || row.querySelector('.lat')?.textContent || '').trim();
    if (!raw || raw === '—' || raw === '…') return { known: false, ms: 0 };
    if (/timeout|fail|error|不可用|aborted/i.test(raw)) return { known: false, ms: 0 };
    const m = String(raw).match(/(-?\d+(?:\.\d+)?)/);
    if (!m) return { known: false, ms: 0 };
    const v = parseFloat(m[1]);
    if (!Number.isFinite(v) || v < 0) return { known: false, ms: 0 };
    return { known: true, ms: v };
  }
  function parseLatMs(row) {
    // legacy numeric helper for filters; untested/timeout → +Infinity (not first)
    const k = parseLatKey(row);
    return k.known ? k.ms : Number.POSITIVE_INFINITY;
  }
  function parseFlowBytes(row) {
    const t = row.querySelector('.flow')?.textContent || '';
    if (!t || t.includes('—')) return -1;
    let total = 0;
    const re = /([\d.]+)\s*([KMG]?B)/gi;
    let m;
    while ((m = re.exec(t))) {
      let n = parseFloat(m[1]);
      const u = m[2].toUpperCase();
      if (u.startsWith('K')) n *= 1024;
      else if (u.startsWith('M')) n *= 1024 * 1024;
      else if (u.startsWith('G')) n *= 1024 * 1024 * 1024;
      total += n;
    }
    return total;
  }
  function sortValue(row, key) {
    switch (key) {
      case 'idx': return parseInt(row.querySelector('.idx')?.textContent || '0', 10) || 0;
      case 'type': return (row.querySelector('.pill')?.textContent || '').toLowerCase();
      case 'addr': return (row.querySelector('.addr')?.textContent || '').toLowerCase();
      case 'name': return (row.dataset.name || row.querySelector('.name')?.textContent || '').toLowerCase();
      case 'lat': return parseLatMs(row);
      case 'flow': return parseFlowBytes(row);
      default: return '';
    }
  }
  function renumberRows() {
    let i = 1;
    nodeTable.querySelectorAll('tr').forEach(r => {
      if (r.style.display === 'none') return;
      const cell = r.querySelector('.idx');
      if (cell) cell.textContent = String(i++);
    });
  }
  function sortNodeTable(key, dir) {
    const rows = [...nodeTable.querySelectorAll('tr')];
    rows.sort((a, b) => {
      let cmp = 0;
      if (key === 'lat') {
        // known ms asc/desc; unknown (—/…/timeout) always last (upstream)
        const ka = parseLatKey(a);
        const kb = parseLatKey(b);
        if (ka.known !== kb.known) cmp = ka.known ? -1 : 1;
        else if (ka.known) cmp = (ka.ms - kb.ms) * dir;
        else cmp = 0;
      } else {
        const va = sortValue(a, key);
        const vb = sortValue(b, key);
        if (typeof va === 'number' && typeof vb === 'number') cmp = (va - vb) * dir;
        else cmp = String(va).localeCompare(String(vb), locale, { numeric: true, sensitivity: 'base' }) * dir;
      }
      if (cmp === 0) {
        const na = a.dataset.name || '';
        const nb = b.dataset.name || '';
        cmp = na.localeCompare(nb, locale);
      }
      return cmp;
    });
    const frag = document.createDocumentFragment();
    rows.forEach(r => frag.appendChild(r));
    nodeTable.appendChild(frag);
    renumberRows();
  }
  function resortIfLatencyActive() {
    if (sortKey === 'lat') sortNodeTable('lat', sortDir);
  }
  function setSortHeader(key, dir) {
    document.querySelectorAll('thead th.sortable').forEach(th => {
      if (th.dataset.sort === key) {
        th.setAttribute('aria-sort', dir === 1 ? 'ascending' : 'descending');
      } else {
        th.removeAttribute('aria-sort');
      }
    });
  }
  document.querySelectorAll('thead th.sortable').forEach(th => {
    th.tabIndex = 0;
    th.setAttribute('role', 'columnheader');
    const doSort = () => {
      const key = th.dataset.sort;
      if (sortKey === key) sortDir = -sortDir;
      else {
        sortKey = key;
        // latency: first click = fastest first (asc); flow: largest first (desc)
        sortDir = (key === 'flow') ? -1 : 1;
      }
      sortNodeTable(sortKey, sortDir);
      setSortHeader(sortKey, sortDir);
    };
    th.addEventListener('click', doSort);
    th.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); doSort(); }
    });
  });

  function now() {
    return new Date().toTimeString().slice(0, 8);
  }

  function logSpan(klass, text) {
    const s = document.createElement('span');
    s.className = klass;
    s.textContent = String(text ?? '');
    return s;
  }

  /** msg is TEXT, never HTML: callers pass node names, share links and backend
   *  error strings, all of which a hostile subscription controls — and this
   *  webview can invoke every Tauri command. Keep this a textContent sink. */
  function log(tag, cls, msg) {
    const line = document.createElement('div');
    line.className = 'log-line';
    const lvl = (cls === 'ok' || cls === 'warn' || cls === 'info') ? cls : 'info';
    line.dataset.lvl = lvl;
    line.append(
      logSpan('log-time', now()),
      logSpan('log-tag ' + cls, tag),
      logSpan('log-msg', msg),
    );
    logPanel.appendChild(line);
    if (typeof applyLogFilter === 'function') applyLogFilter();
    logPanel.scrollTop = logPanel.scrollHeight;
  }

  function setConnected(on, opts) {
    const prev = !!connected;
    const side = !opts || opts.sideEffects !== false;
    connected = !!on;
    const pb = document.getElementById('powerBtn') || powerBtn;
    pb.classList.toggle('on', connected);
    pb.setAttribute('aria-pressed', String(connected));
    statusDot.classList.toggle('on', connected);
    // traffic: never invent; disconnect keeps per-node totals (only 重置流量 zeros)
    if (!connected) {
      if (typeof refreshSbProxyFromNodes === 'function') refreshSbProxyFromNodes();
      else if (sbProxy) sbProxy.textContent = '—';
      const sbDirect = document.getElementById('sbDirect');
      if (sbDirect) sbDirect.textContent = '—';
      statsStarted = null;
      connectedName = '';
      connectedLat = '—';
    } else {
      if (statsStarted == null) statsStarted = Date.now();
      // pin hero to the node that actually started (unless caller only refreshes chrome)
      if (!opts || opts.pin !== false) {
        connectedName = selectedName;
        connectedLat = (selectedLat && selectedLat !== '—' && selectedLat !== '…') ? selectedLat : '—';
      }
      if (typeof refreshSbProxyFromNodes === 'function') refreshSbProxyFromNodes();
    }
    refreshHeroStatus();
    refreshConnectedRow();
    // Poll only on false↔true edge (renderNodes must not rebaseline traffic).
    if (side) {
      if (!prev && connected) {
        if (typeof startConnPoll === 'function') startConnPoll();
      } else if (prev && !connected) {
        if (typeof stopConnPoll === 'function') stopConnPoll();
      }
    }
    if (typeof refreshTrayMenu === 'function') refreshTrayMenu();
  }
  /** Mark the live tunnel row (green); independent of multi-select blue. */
  function refreshConnectedRow() {
    if (!nodeTable) return;
    const live = (connected && connectedName) ? connectedName : '';
    nodeTable.querySelectorAll('tr.connected').forEach(r => r.classList.remove('connected'));
    if (!live) return;
    const row = [...nodeTable.querySelectorAll('tr')].find(r => r.dataset.name === live);
    if (row) row.classList.add('connected');
  }
  /** Connected line = tunnel node; disconnected line = list selection. */
  function setPowerBusy(on, wantConnect) {
    powerBusy = !!on;
    const pb = document.getElementById('powerBtn') || powerBtn;
    if (!pb) return;
    pb.disabled = !!on;
    pb.setAttribute('aria-busy', on ? 'true' : 'false');
    if (typeof refreshHeroStatus === 'function') refreshHeroStatus(wantConnect);
  }

  function refreshHeroStatus(busyWantConnect) {
    const via = connectedName || selectedName;
    const latShow = connected
      ? ((connectedLat && connectedLat !== '—' && connectedLat !== '…') ? connectedLat : '—')
      : ((selectedLat && selectedLat !== '—' && selectedLat !== '…') ? selectedLat : '—');
    const mismatch = !!(connected && connectedName && selectedName && selectedName !== '—' && selectedName !== connectedName);
    const busy = !!powerBusy;
    const wantOn = busyWantConnect != null ? !!busyWantConnect : !connected;
    if (busy) {
      statusText.textContent = wantOn ? t('status.connecting') : t('status.disconnecting');
      statusSub.innerHTML = wantOn
        ? tHtml('status.subConnecting', { name: selectedName || via || '—' })
        : tHtml('status.subDisconnecting', { name: connectedName || via || '—' });
      sbStatus.textContent = t('sb.busy');
    } else {
      statusText.textContent = connected ? t('status.connected') : t('status.disconnected');
      if (connected && mismatch) {
        statusSub.innerHTML = tHtml('status.subMismatch', { tunnel: connectedName, selected: selectedName, lat: latShow });
      } else {
        statusSub.innerHTML = connected
          ? tHtml('status.subOn', { name: via, lat: latShow })
          : tHtml('status.subOff', { name: selectedName });
      }
      sbStatus.textContent = connected ? t('sb.running') : t('sb.stopped');
    }
  }

  async function nexusInvoke(cmd, args) {
    try {
      const inv = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
      if (!inv) return { ok: false, offline: true };
      const data = await inv(cmd, args || {});
      return { ok: true, data };
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  }

  // Session op queue: one promise chain; coalesce reconnect-class ops to latest.
  let sessionOpChain = Promise.resolve();
  let sessionOpPending = null; // {kind, run}
  let sessionOpRunning = false;
  function runSessionOp(kind, fn) {
    // coalesce: only one pending reconnect/disconnect of same class
    if (kind === 'reconnect' || kind === 'disconnect') {
      if (sessionOpRunning && sessionOpPending && sessionOpPending.kind === kind) {
        sessionOpPending = { kind, run: fn };
        return sessionOpChain;
      }
      if (!sessionOpRunning && sessionOpPending && sessionOpPending.kind === kind) {
        sessionOpPending = { kind, run: fn };
        return sessionOpChain;
      }
      sessionOpPending = { kind, run: fn };
      sessionOpChain = sessionOpChain.then(async () => {
        while (sessionOpPending) {
          const job = sessionOpPending;
          sessionOpPending = null;
          sessionOpRunning = true;
          try { await job.run(); } catch (e) {
            try { log('SYS', 'warn', String(e && e.message || e)); } catch (_) {}
          } finally { sessionOpRunning = false; }
        }
      });
      return sessionOpChain;
    }
    // generic serial
    sessionOpChain = sessionOpChain.then(async () => {
      sessionOpRunning = true;
      try { await fn(); } finally { sessionOpRunning = false; }
    });
    return sessionOpChain;
  }
  let catalogPutChain = Promise.resolve();
  let _catalogUnloadBound = false;
  function flushCatalogPut() {
    // 3A: store.json is sole truth. LS only as offline/bridge-down cache.
    const blob = serializeCatalog();
    if (typeof nexusInvoke !== 'function') {
      try { localStorage.setItem(CATALOG_KEY, JSON.stringify(blob)); } catch (_) {}
      return Promise.resolve();
    }
    return nexusInvoke('catalog_put', { blob }).then(r => {
      if (r && r.ok) {
        try { localStorage.removeItem(CATALOG_KEY); } catch (_) {}
        return;
      }
      // Put failed / offline — keep LS so next boot can migrate.
      try { localStorage.setItem(CATALOG_KEY, JSON.stringify(blob)); } catch (_) {}
      if (r && !r.ok && !r.offline) {
        try { log('SYS', 'warn', t('log.subStoreFail', { error: r.error || '' })); } catch (_) {}
      }
    }).catch(e => {
      try { localStorage.setItem(CATALOG_KEY, JSON.stringify(blob)); } catch (_) {}
      try { log('SYS', 'warn', t('log.subStoreFail', { error: e && e.message || e })); } catch (_) {}
    });
  }

  powerBtn.addEventListener('click', () => {
    // Always queue/coalesce — never drop a connect/disconnect intent.
    const wantConnect = !connected;
    const kind = wantConnect ? 'reconnect' : 'disconnect';
    runSessionOp(kind, async () => {
      setPowerBusy(true, wantConnect);
      try {
        if (wantConnect) {
          // Power with no row selected: use first listed node.
          if ((!selectedName || selectedName === '—') && typeof findNodeByName === 'function') {
            const first = document.querySelector('#nodeTable tr[data-name]');
            if (first && first.dataset.name) {
              selectedName = first.dataset.name;
              selectedLat = first.dataset.lat || '—';
              if (typeof selectNodeRow === 'function') selectNodeRow(first);
            }
          }
          const payload = (typeof nodeConnectPayload === 'function')
            ? nodeConnectPayload(selectedName)
            : null;
          log('CORE', 'info', t('log.startProfile', { name: selectedName }));
          if (!payload) {
            const n = typeof findNodeByName === 'function' ? findNodeByName(selectedName) : null;
            if (n && n.link && !isShareUri(n.link)) {
              log('CORE', 'warn', t('log.badShare'));
            } else if (n && n.addr && n.addr !== '—') {
              log('CORE', 'warn', t('log.noCreds'));
            } else {
              log('CORE', 'warn', t('log.noRealLink'));
            }
            return;
          }
          const connectArgs = Object.assign({
            profile_id: 1,
            tun: typeof isTunOn === 'function' ? isTunOn() : !!document.getElementById('tunToggle')?.checked,
            system_proxy: !!document.getElementById('sysToggle')?.checked,
          }, payload);
          const r = await connectSelectedWithHelper(connectArgs);
          if (typeof refreshFirewall === 'function') refreshFirewall();
          if (!r || r.offline) {
            log('CORE', 'warn', t('log.backendDown'));
            return;
          }
          if (!r.ok) { log('CORE', 'warn', t('log.startFail', { error: r.error })); return; }
          if (r.data?.start_error) {
            log('CORE', 'warn', `Start: ${r.data.start_error}`);
            // Start failed after Connecting/Blocked — open network so user can fix node/DNS.
            try { await nexusInvoke('disconnect_selected'); } catch (_) {}
            if (typeof refreshFirewall === 'function') refreshFirewall();
            return;
          }
          if (!r.data?.started) {
            log('CORE', 'warn', t('log.startNoOk'));
            try { await nexusInvoke('disconnect_selected'); } catch (_) {}
            if (typeof refreshFirewall === 'function') refreshFirewall();
            return;
          }
          setConnected(true);
          try { localStorage.setItem('nexus.lastConnected', connectedName || selectedName || ''); } catch (_) {}
          const port = r.data?.listen_port || mixedPort();
          log('OK', 'ok', t('log.tunnelOn', { port, name: connectedName || selectedName }));
          if (r.data?.proxy_note) log('SYS', 'info', r.data.proxy_note);
          if (r.data?.tun && r.data?.tun_if) log('SYS', 'info', `Tun iface: ${r.data.tun_if}`);
          else if (r.data?.tun) log('SYS', 'warn', 'Tun on — waiting for utun (firewall rebind)');
          if (typeof fillRuntimeStats === 'function') fillRuntimeStats();
        } else {
          log('CORE', 'warn', t('log.stopProfile', { name: connectedName || selectedName }));
          let r = await nexusInvoke('disconnect_selected');
          if (r && !r.ok && String(r.error || '').includes('core not started')) {
            r = await nexusInvoke('core_stop');
          } else if (r && !r.ok) {
            await nexusInvoke('core_stop');
          }
          if (typeof refreshFirewall === 'function') refreshFirewall();
          setConnected(false);
          try { localStorage.removeItem('nexus.lastConnected'); } catch (_) {}
          log('OK', 'ok', t('log.stopped'));
          const disc = r && r.ok ? r.data : r;
          if (disc?.firewall_error) log('SYS', 'warn', disc.firewall_error);
          else if (disc?.proxy_note) log('SYS', 'info', disc.proxy_note);
          if (typeof fillRuntimeStats === 'function') fillRuntimeStats();
        }
      } finally {
        setPowerBusy(false);
      }
    });
  });
  // nexusInvoke defined later; hoist stub
  

  // progressive selection: click = single; ⌘/Ctrl = toggle; Shift = range from anchor
  let selectAnchorRow = null;
  /** Live tr for range select — rebind by name after renderNodes/sort. */
  function resolveSelectAnchor(rows) {
    if (selectAnchorRow && rows.includes(selectAnchorRow)) return selectAnchorRow;
    const name = selectAnchorRow?.dataset?.name || selectedName;
    if (name) {
      const byName = rows.find(r => r.dataset.name === name);
      if (byName) {
        selectAnchorRow = byName;
        return byName;
      }
    }
    const sel = rows.find(r => r.classList.contains('selected'));
    if (sel) {
      selectAnchorRow = sel;
      return sel;
    }
    return null;
  }
  function selectNodeRow(row, { multi, range } = {}) {
    if (!row) return;
    const rows = visibleNodeRows();
    if (range) {
      const anchor = resolveSelectAnchor(rows) || row;
      const a = rows.indexOf(anchor);
      const b = rows.indexOf(row);
      if (a >= 0 && b >= 0) {
        const lo = Math.min(a, b), hi = Math.max(a, b);
        if (!multi) rows.forEach(r => r.classList.remove('selected'));
        for (let i = lo; i <= hi; i++) rows[i].classList.add('selected');
        // keep original anchor (Finder-style); seed if none
        if (!selectAnchorRow || !rows.includes(selectAnchorRow)) selectAnchorRow = anchor;
      } else {
        if (!multi) rows.forEach(r => r.classList.remove('selected'));
        row.classList.add('selected');
        selectAnchorRow = row;
      }
    } else if (multi) {
      row.classList.toggle('selected');
      selectAnchorRow = row;
    } else {
      rows.forEach(r => r.classList.remove('selected'));
      row.classList.add('selected');
      selectAnchorRow = row;
    }
    const sel = row.classList.contains('selected') ? row : nodeTable.querySelector('tr.selected');
    if (sel) {
      selectedName = sel.dataset.name;
      selectedLat = sel.dataset.lat || sel.querySelector('.lat')?.textContent || '—';
      // Connected hero stays on tunnel node until Start/Stop; only selection changes here.
      if (typeof refreshHeroStatus === 'function') refreshHeroStatus();
      else if (!connected) {
        statusSub.innerHTML = tHtml('status.subOff', { name: selectedName });
      }
    }
  }

  // Kill native text selection in the node list (Shift otherwise selects glyphs, not rows)
  function clearDomTextSelection() {
    try { window.getSelection()?.removeAllRanges(); } catch (_) { /* ignore */ }
  }
  nodeTable.addEventListener('selectstart', (e) => { e.preventDefault(); }, true);
  nodeTable.addEventListener('dragstart', (e) => { e.preventDefault(); }, true);
  nodeTable.addEventListener('mousedown', (e) => {
    if (!e.target.closest('tr')) return;
    e.preventDefault(); // always — stops caret + Shift text-range
    clearDomTextSelection();
  }, true);
  nodeTable.addEventListener('click', (e) => {
    const row = e.target.closest('tr');
    if (!row || !nodeTable.contains(row)) return;
    if (e.shiftKey || e.metaKey || e.ctrlKey) clearDomTextSelection();
    const multi = e.metaKey || e.ctrlKey;
    const range = e.shiftKey;
    selectNodeRow(row, { multi, range });
    nodeListFocus = true;
    tableCard?.focus({ preventScroll: true });
    // click alone does not hot-switch Core — no "切换节点" log (Start/ctx start does)
  });

  const ctxMenu = document.getElementById('ctxMenu');
  function closeCtxMenu() {
    if (!ctxMenu) return;
    ctxMenu.classList.remove('open');
    ctxMenu.hidden = true;
  }
  function openCtxMenu(x, y) {
    if (!ctxMenu) return;
    closeMenus();
    // menu_update_subscription enabled only when group has url
    const gid = (typeof activeGroupId === 'function') ? activeGroupId() : 'default';
    const g = (typeof GROUPS !== 'undefined') ? GROUPS.find(x => x.id === gid) : null;
    const hasUrl = !!(g && g.url && String(g.url).trim());
    const hasRows = selectedRows().length > 0 || nodeTable.querySelectorAll('tr').length > 0;
    ctxMenu.querySelectorAll('[data-ctx]').forEach(b => {
      const act = b.dataset.ctx;
      let en = true;
      if (act === 'refresh-sub') en = hasUrl && !subUpdating;
      if (['edit', 'start', 'clone', 'copy-link', 'show-qr'].includes(act)) en = !!selectedNames().length || !!document.querySelector('#nodeTable tr.selected');
      if (['delete', 'url-test', 'resolve-ip'].includes(act)) en = selectedRows().length > 0;
      if (['stop'].includes(act)) en = !!connected;
      b.disabled = !en;
      b.toggleAttribute('disabled', !en);
    });
    ctxMenu.hidden = false;
    ctxMenu.classList.add('open');
    const w = ctxMenu.offsetWidth || 220;
    const h = ctxMenu.offsetHeight || 320;
    const left = Math.min(x, window.innerWidth - w - 8);
    const top = Math.min(y, window.innerHeight - h - 8);
    ctxMenu.style.left = Math.max(8, left) + 'px';
    ctxMenu.style.top = Math.max(8, top) + 'px';
  }
  function selectedRows() {
    return [...nodeTable.querySelectorAll('tr.selected')];
  }
  function selectedNames() {
    return selectedRows().map(r => r.dataset.name).filter(Boolean);
  }
  function visibleNodeRows() {
    return [...nodeTable.querySelectorAll('tr')].filter(r => r.style.display !== 'none');
  }
  function selectAllNodes() {
    const rows = visibleNodeRows();
    rows.forEach(r => r.classList.add('selected'));
    const first = rows[0];
    if (first) {
      selectedName = first.dataset.name || selectedName;
      selectedLat = first.dataset.lat || first.querySelector('.lat')?.textContent || selectedLat;
      selectAnchorRow = first;
    }
    log('SYS', 'info', t('log.selectedN', { n: rows.length }));
    return rows.length;
  }

  // right-click on table body / list area
  const listWrap = document.querySelector('.list-wrap');
  listWrap?.addEventListener('pointerenter', () => { nodeListPointerInside = true; });
  listWrap?.addEventListener('pointerleave', () => { nodeListPointerInside = false; });
  listWrap?.addEventListener('mousedown', (e) => {
    if (e.target.closest('.search')) return;
    nodeListFocus = true;
  }, true);
  document.addEventListener('mousedown', (e) => {
    if (!e.target.closest('.list-wrap')) nodeListFocus = false;
  }, true);
  listWrap?.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    const row = e.target.closest('#nodeTable tr');
    if (row) {
      if (!row.classList.contains('selected')) selectNodeRow(row);
    }
    openCtxMenu(e.clientX, e.clientY);
  });
  document.addEventListener('click', (e) => {
    if (!e.target.closest('#ctxMenu')) closeCtxMenu();
    if (!e.target.closest('#logCtxMenu')) closeLogCtxMenu?.();
    if (!e.target.closest('#connCtxMenu')) closeConnCtxMenu?.();
  });
  function isEditableTarget(el) {
    if (!el || el === document.body) return false;
    const tag = (el.tagName || '').toUpperCase();
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
    if (el.isContentEditable) return true;
    return !!el.closest?.('input, textarea, select, [contenteditable="true"]');
  }
  function nodeListHotkeyActive() {
    const ae = document.activeElement;
    if (isEditableTarget(ae) && !ae?.closest?.('.table-card')) return false;
    if (ae && (ae === tableCard || ae.closest?.('.table-card') || ae.closest?.('#nodeTable'))) return true;
    if (nodeListPointerInside || nodeListFocus) return true;
    return false;
  }
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') closeCtxMenu();
    // ⌘A / Ctrl+A: select all nodes when pointer/focus is in node list
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && (e.key === 'a' || e.key === 'A')) {
      if (nodeListHotkeyActive()) {
        e.preventDefault();
        e.stopPropagation();
        selectAllNodes();
        tableCard?.focus({ preventScroll: true });
      }
    }
  });
  window.addEventListener('resize', () => closeCtxMenu());
  document.querySelector('.table-scroll')?.addEventListener('scroll', () => closeCtxMenu(), { passive: true });

  document.querySelectorAll('#ctxMenu [data-ctx]').forEach(btn => {
    btn.addEventListener('click', () => {
      const act = btn.dataset.ctx;
      closeCtxMenu();
      const names = selectedNames();
      const one = names[0] || selectedName;
      const n = names.length || 1;
      const map = {
        'add-clip': () => { importFromClipboard(); },
        'add-file': () => { importFromFile(); },
        'scan-qr': () => { importScanQr(); },
        start: () => {
          if (one) {
            selectedName = one;
            const row = [...nodeTable.querySelectorAll('tr')].find(r => r.dataset.name === one);
            if (row) selectedLat = row.dataset.lat || '—';
          }
          runSessionOp('reconnect', async () => {
            setPowerBusy(true, true);
            try {
              if (connected) {
                const dr = await nexusInvoke('disconnect_selected');
                if (dr && !dr.ok) await nexusInvoke('core_stop');
                const dd = dr && dr.ok ? dr.data : dr;
                if (dd?.firewall_error) log('SYS', 'warn', dd.firewall_error);
                setConnected(false);
              }
              const payload = (typeof nodeConnectPayload === 'function')
                ? nodeConnectPayload(selectedName) : null;
              if (!payload) {
                log('CORE', 'warn', t('log.noRealLink'));
                return;
              }
              log('CORE', 'info', t('log.startProfile', { name: selectedName }));
              const r = await connectSelectedWithHelper(Object.assign({
                profile_id: 1,
                tun: typeof isTunOn === 'function' ? isTunOn() : !!document.getElementById('tunToggle')?.checked,
                system_proxy: !!document.getElementById('sysToggle')?.checked,
              }, payload));
              if (!r || r.offline) { log('CORE', 'warn', t('log.backendDown')); return; }
              if (!r.ok) { log('CORE', 'warn', t('log.startFail', { error: r.error })); return; }
              if (r.data?.start_error) {
                log('CORE', 'warn', `Start: ${r.data.start_error}`);
                try { await nexusInvoke('disconnect_selected'); } catch (_) {}
                if (typeof refreshFirewall === 'function') refreshFirewall();
                return;
              }
              if (!r.data?.started) {
                log('CORE', 'warn', t('log.startNoOk'));
                try { await nexusInvoke('disconnect_selected'); } catch (_) {}
                if (typeof refreshFirewall === 'function') refreshFirewall();
                return;
              }
              setConnected(true);
              try { localStorage.setItem('nexus.lastConnected', connectedName || selectedName || ''); } catch (_) {}
              log('OK', 'ok', t('log.tunnelOn', { port: r.data?.listen_port || mixedPort(), name: connectedName || selectedName }));
            } finally {
              setPowerBusy(false);
            }
          });
        },
        stop: () => {
          if (connected) powerBtn.click();
          else log('SYS', 'info', t('log.notConnected'));
        },
        delete: async () => {
          const rows = selectedRows();
          if (!rows.length) { log('SYS', 'warn', t('log.noNode')); return; }
          if (rows.length > 1) {
            const msg = t('confirm.deleteNodes', { n: rows.length });
            const ok = await askConfirm(msg, {
              title: t('confirm.deleteNodesTitle'),
              okText: t('ctx.delete'),
              danger: true,
            });
            if (!ok) return;
          }
          // re-query selection after await (DOM may still match)
          const live = selectedRows().length ? selectedRows() : rows;
          const names = new Set(live.map(r => r.dataset.name));
          const gid = currentGid();
          const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
          if (prof && Array.isArray(prof.nodes)) {
            prof.nodes = prof.nodes.filter(n => !names.has(n.name));
            const g = GROUPS.find(x => x.id === gid);
            if (g) g.count = prof.nodes.length;
          }
          live.forEach(r => r.remove());
          renumberRows();
          if (typeof saveCatalog === 'function') saveCatalog(true);
          log('SYS', 'warn', t('log.deletedN', { n: names.size }));
        },
        'select-all': () => { selectAllNodes(); },
        'refresh-sub': () => { updateSubscription(null); },
        'url-test': () => { runUrlTest('selected'); },
        'resolve-ip': () => { resolveSelectedIps(); },
        'clear-test': () => { clearTestResults(); },
        'reset-traffic': () => {
          // on_menu_reset_traffic: selected profiles only (explicit zero — switch must not)
          const rows = selectedRows();
          const targets = rows.length ? rows : [...nodeTable.querySelectorAll('tr')];
          const names = new Set(targets.map(r => r.dataset.name));
          targets.forEach(r => {
            const el = r.querySelector('.flow');
            if (el) {
              el.textContent = '—';
              el.classList.remove('flow');
              el.classList.add('flow', 'muted');
            }
          });
          if (typeof SUB_PROFILES !== 'undefined') {
            for (const id of Object.keys(SUB_PROFILES || {})) {
              const nodes = SUB_PROFILES[id]?.nodes;
              if (!Array.isArray(nodes)) continue;
              nodes.forEach(n => {
                if (!names.has(n.name)) return;
                n.flow = null;
                n.flowUp = 0;
                n.flowDown = 0;
              });
            }
          }
          // re-baseline Core so next poll doesn't re-add pre-reset session bytes
          _coreBaseUp = null;
          _coreBaseDown = null;
          if (!rows.length) {
            if (sbProxy) sbProxy.textContent = connected ? '0 B' : '—';
            const sbDirect = document.getElementById('sbDirect');
            if (sbDirect) sbDirect.textContent = '—';
          } else if (typeof refreshSbProxyFromNodes === 'function') {
            refreshSbProxyFromNodes();
          }
          log('SYS', 'info', t('log.resetTraffic', { n: targets.length }));
        },
        edit: () => openEditDialog(one),
        clone: () => {
          const row = selectedRows()[0];
          if (!row) { log('SYS', 'warn', t('log.noNode')); return; }
          const name = t('js.nodeCopy', { name: row.dataset.name || t('js.nodes') });
          const gid = currentGid();
          const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
          if (prof && Array.isArray(prof.nodes)) {
            const src = prof.nodes.find(n => n.name === row.dataset.name);
            const copy = src
              ? { ...src, name }
              : {
                  name,
                  type: row.querySelector('.pill')?.textContent || 'VLESS',
                  addr: row.querySelector('.addr')?.textContent || '—',
                  lat: null,
                  flow: null,
                };
            const idx = prof.nodes.findIndex(n => n.name === row.dataset.name);
            prof.nodes.splice(idx >= 0 ? idx + 1 : prof.nodes.length, 0, copy);
            const g = GROUPS.find(x => x.id === gid);
            if (g) g.count = prof.nodes.length;
          }
          const clone = row.cloneNode(true);
          clone.classList.remove('selected');
          clone.dataset.name = name;
          const nameEl = clone.querySelector('.name');
          if (nameEl) nameEl.textContent = name;
          row.after(clone);
          renumberRows();
          if (typeof saveCatalog === 'function') saveCatalog(true);
          log('SYS', 'ok', t('log.cloned', { name }));
        },
        'copy-link': () => {
          const link = nodeShareLink(one);
          copyText(link).then(() => log('SYS', 'ok', t('log.copiedLinkNamed', { name: one }))).catch(() => log('SYS', 'ok', t('log.linkIs', { link })));
        },
        'show-qr': () => openQrDialog(one),
        dedupe: () => {
          // on_menu_delete_repeat: Uniq by profile identity (addr|type)
          const seen = new Set();
          const drop = [];
          [...nodeTable.querySelectorAll('tr')].forEach(r => {
            const key = (r.querySelector('.addr')?.textContent || '') + '|' + (r.querySelector('.pill')?.textContent || '');
            if (seen.has(key)) drop.push(r);
            else seen.add(key);
          });
          if (!drop.length) { log('SYS', 'info', t('log.noDupes')); return; }
          const names = new Set(drop.map(r => r.dataset.name));
          const gid = currentGid();
          const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
          if (prof?.nodes) {
            const keep = new Set();
            const next = [];
            for (const n of prof.nodes) {
              const k = (n.addr || '') + '|' + (n.type || '');
              if (keep.has(k)) continue;
              keep.add(k);
              next.push(n);
            }
            prof.nodes = next;
            const g = GROUPS.find(x => x.id === gid);
            if (g) g.count = next.length;
          }
          drop.forEach(r => r.remove());
          renumberRows();
          if (typeof saveCatalog === 'function') saveCatalog(true);
          log('SYS', 'ok', t('log.deduped', { n: drop.length }));
        },
        'rm-unavailable': () => {
          // clearUnavailableProfiles: latency == -1 (test failed)
          removeNodesByPred(r => {
            const t = (r.dataset.lat || r.querySelector('.lat')?.textContent || '').trim();
            const ms = parseLatMs(r);
            return t === 'timeout' || /fail|error|timeout|不可用/i.test(t) || ms === -1 || (r.querySelector('.lat')?.classList.contains('bad') && ms < 0);
          }, t('js.rmUnavailableDone'));
        },
        'rm-failed': () => {
          // remove invalid / no latency: empty or failed test result
          removeNodesByPred(r => {
            const t = (r.dataset.lat || r.querySelector('.lat')?.textContent || '').trim();
            if (!t || t === '—' || t === '…') return true;
            if (/fail|error|timeout/i.test(t)) return true;
            const ms = parseLatMs(r);
            return ms < 0;
          }, t('js.rmFailedDone'));
        },
      };
      (map[act] || (() => log('SYS', 'info', act)))();
    });
  });

  // double-click row → edit
  nodeTable.addEventListener('dblclick', (e) => {
    const row = e.target.closest('tr');
    if (!row) return;
    selectNodeRow(row);
    openEditDialog(row.dataset.name || selectedName);
  });

  function setChipOn(toggleId, chipId, on, mirrorId) {
    const t = document.getElementById(toggleId);
    if (t) t.checked = !!on;
    document.getElementById(chipId)?.classList.toggle('on', !!on);
    if (mirrorId) {
      const s = document.getElementById(mirrorId);
      if (s) { s.classList.toggle('on', !!on); s.setAttribute('aria-pressed', String(!!on)); }
    }
  }
  function bindChip(toggleId, chipId, onMsg, offMsg, mirrorId) {
    const t = document.getElementById(toggleId);
    if (!t) return;
    t.addEventListener('change', (e) => {
      document.getElementById(chipId)?.classList.toggle('on', e.target.checked);
      if (mirrorId) {
        const s = document.getElementById(mirrorId);
        if (s) { s.classList.toggle('on', e.target.checked); s.setAttribute('aria-pressed', String(e.target.checked)); }
      }
      log('SYS', 'info', e.target.checked ? onMsg : offMsg);
    });
  }
  function isTunOn() { return !!document.getElementById('tunToggle')?.checked; }
  function applyTun(on) {
    setChipOn('tunToggle', 'tunChip', on);
    log('SYS', 'info', on ? t('log.tunOn') : t('log.tunOff'));
    if (typeof refreshTrayMenu === 'function') refreshTrayMenu();
  }
  document.getElementById('tunToggle')?.addEventListener('change', (e) => {
    applyTun(!!e.target.checked);
  });


  // test menu (slim) — open/close wired after bindMenu; handlers here

  function removeNodesByPred(pred, logLabel) {
    const rows = [...nodeTable.querySelectorAll('tr')].filter(pred);
    if (!rows.length) { log('SYS', 'info', t('log.removed0', { label: logLabel || t('log.remove') })); return 0; }
    const names = new Set(rows.map(r => r.dataset.name).filter(Boolean));
    const gid = typeof currentGid === 'function' ? currentGid() : 'default';
    const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
    if (prof && Array.isArray(prof.nodes)) {
      prof.nodes = prof.nodes.filter(n => !names.has(n.name));
      const g = GROUPS.find(x => x.id === gid);
      if (g) g.count = prof.nodes.length;
    }
    rows.forEach(r => r.remove());
    renumberRows();
    if (typeof saveCatalog === 'function') saveCatalog(true);
    log('SYS', 'warn', t('log.removedN', { label: logLabel || t('log.removed'), n: rows.length }));
    return rows.length;
  }

  // runURLTest → Core Test RPC + QueryURLTest poller paints latency as results arrive.
  // Until Start()+Test RPC: real TCP connect RTT; progressive via `net-probe-result` events.
  let urlTesting = false;
  let urlTestAbort = false;
  let urlTestUnlisten = null;
  function tauriInvoke(cmd, args) {
    const inv = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
    if (typeof inv !== 'function') return Promise.reject(new Error('not in Tauri'));
    return inv(cmd, args || {});
  }
  async function tauriListen(event, handler) {
    const listen = window.__TAURI__?.event?.listen
      || window.__TAURI_INTERNALS__?.plugins?.event?.listen
      || null;
    if (typeof listen === 'function') {
      return listen(event, (e) => handler(e?.payload !== undefined ? e.payload : e));
    }
    // Tauri 2 withInternals fallback: window.__TAURI__.event
    try {
      const mod = window.__TAURI__?.event;
      if (mod && typeof mod.listen === 'function') {
        return mod.listen(event, (e) => handler(e?.payload !== undefined ? e.payload : e));
      }
    } catch (_) {}
    return null;
  }
  function parseRowTarget(row) {
    const addr = (row.querySelector('.addr')?.textContent || row.dataset.addr || '').trim();
    if (!addr || addr === '—') return null;
    // host:port or [v6]:port
    let host = '', port = 443;
    if (addr.startsWith('[')) {
      const m = addr.match(/^\[([^\]]+)\]:(\d+)$/);
      if (!m) return null;
      host = m[1]; port = parseInt(m[2], 10) || 443;
    } else {
      const i = addr.lastIndexOf(':');
      if (i <= 0) { host = addr; port = 443; }
      else { host = addr.slice(0, i); port = parseInt(addr.slice(i + 1), 10) || 443; }
    }
    if (!host) return null;
    return { id: row.dataset.name || addr, host, port, row };
  }
  // Throne-style: paint latency immediately; re-sort at most ~5×/s (not every probe).
  let latencyResortTimer = 0;
  let latencyResortRaf = 0;
  function scheduleLatencyResort() {
    if (sortKey !== 'lat') return;
    if (latencyResortTimer) return;
    latencyResortTimer = setTimeout(() => {
      latencyResortTimer = 0;
      if (latencyResortRaf) cancelAnimationFrame(latencyResortRaf);
      latencyResortRaf = requestAnimationFrame(() => {
        latencyResortRaf = 0;
        if (typeof sortNodeTable === 'function' && sortKey === 'lat') {
          sortNodeTable('lat', sortDir);
        }
      });
    }, 200);
  }
  function flushLatencyResort() {
    if (latencyResortTimer) { clearTimeout(latencyResortTimer); latencyResortTimer = 0; }
    if (latencyResortRaf) { cancelAnimationFrame(latencyResortRaf); latencyResortRaf = 0; }
    if (typeof sortNodeTable === 'function' && sortKey === 'lat') {
      sortNodeTable('lat', sortDir);
    }
  }
  function applyLatencyToRow(row, ms, err) {
    const lat = row.querySelector('.lat');
    const name = row.dataset.name;
    const gid = typeof currentGid === 'function' ? currentGid() : 'default';
    const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
    const node = prof?.nodes?.find(n => n.name === name);
    if (err || ms == null || ms < 0) {
      if (lat) { lat.textContent = 'timeout'; lat.className = 'lat bad'; }
      row.dataset.lat = 'timeout';
      if (node) node.lat = -1;
    } else {
      if (lat) {
        lat.textContent = ms + ' ms';
        lat.className = 'lat ' + (ms < 120 ? 'good' : ms < 220 ? 'mid' : 'bad');
      }
      row.dataset.lat = ms + ' ms';
      if (node) node.lat = ms;
    }
    scheduleLatencyResort();
  }
  async function runUrlTest(scope) {
    if (urlTesting) {
      log('TEST', 'warn', t('log.testBusy'));
      return;
    }
    const rows = scope === 'group'
      ? [...nodeTable.querySelectorAll('tr')].filter(r => r.style.display !== 'none')
      : selectedRows();
    const targets = [];
    for (const r of rows) {
      const t = parseRowTarget(r);
      if (t) targets.push(t);
    }
    if (!targets.length) { log('TEST', 'warn', t('log.noTestTargets')); return; }

    // via_node_mode := session_status.running only (eng lock A / AGENTS consensus)
    let coreRunning = false;
    try {
      const st = await tauriInvoke('session_status');
      coreRunning = !!(st && st.running);
    } catch (_) { coreRunning = false; }

    if (coreRunning) {
      // running: hard ban net_tcp_probe; only current connected row via Core TestCurrent
      const onlyCurrent = targets.length === 1
        && connected
        && connectedName
        && targets[0].id === connectedName;
      if (!onlyCurrent) {
        log('TEST', 'warn', t('log.testViaNodeOnly'));
        try {
          await askConfirm(t('log.testViaNodeOnly'), {
            title: t('log.testViaNode'),
            okText: t('btn.ok') || 'OK',
          });
        } catch (_) {}
        return;
      }
      const t0 = targets[0];
      urlTesting = true;
      urlTestAbort = false;
      const label = t('log.testViaNode');
      log('TEST', 'info', t('log.testStartViaNode', { name: connectedName }));
      const lat = t0.row.querySelector('.lat');
      if (lat) { lat.textContent = '…'; lat.className = 'lat muted'; }
      t0.row.dataset.lat = '…';
      let ok = 0, fail = 0;
      try {
        let res;
        try {
          res = await tauriInvoke('core_url_test_current', {
            url: 'https://www.gstatic.com/generate_204',
            timeoutMs: 3000,
          });
        } catch (e) {
          log('TEST', 'warn', t('log.probeUnavailable', { error: e && e.message || e }));
          applyLatencyToRow(t0.row, -1, true);
          fail = 1;
          log('OK', 'ok', t('log.testDone', { label, ok, fail, extra: urlTestAbort ? t('log.testStopped') : '' }));
          return;
        }
        // paint only connectedName row; ignore Core OutboundTag
        const rows = res?.results || [];
        const r = rows[0] || {};
        const err = (r.error || '').trim();
        const ms = typeof r.ms === 'number' ? r.ms : 0;
        // never paint 0/error as green
        if (urlTestAbort || err || ms <= 0) {
          applyLatencyToRow(t0.row, -1, true);
          fail = 1;
          if (err && err !== 'test aborted' && !urlTestAbort) {
            log('TEST', 'warn', `[${connectedName}] ${err}`);
          }
        } else {
          applyLatencyToRow(t0.row, ms, false);
          ok = 1;
        }
        flushLatencyResort();
        log('OK', 'ok', t('log.testDone', {
          label,
          ok,
          fail,
          extra: urlTestAbort ? t('log.testStopped') : '',
        }));
        log('SYS', 'info', t('log.testNoteViaNode'));
        if (typeof saveCatalog === 'function') saveCatalog(true);
      } finally {
        urlTesting = false;
      }
      return;
    }

    // !running: TCP direct progressive (reachability only)
    urlTesting = true;
    urlTestAbort = false;
    const label = scope === 'group' ? t('test.urlGroup') : t('test.urlSelected');
    // concurrency scales with batch size (Throne-like); cap 64 for free-list comfort
    const conc = Math.min(64, Math.max(16, Math.ceil(targets.length / 6)));
    log('TEST', 'info', t('log.testStart', { label, n: targets.length }));
    // mark pending + index by id for progressive paint
    const byId = new Map();
    for (const t of targets) {
      byId.set(t.id, t);
      const lat = t.row.querySelector('.lat');
      if (lat) { lat.textContent = '…'; lat.className = 'lat muted'; }
      t.row.dataset.lat = '…';
    }
    let ok = 0, fail = 0, done = 0;
    const total = targets.length;
    const seen = new Set();
    let lastProgressLog = 0;
    const onResult = (payload) => {
      // harden unwrap (same as resolve path) — progressive must not depend on final batch
      const r = (payload && payload.payload !== undefined) ? payload.payload : payload;
      if (!r || r.id == null) return;
      if (seen.has(r.id)) return;
      seen.add(r.id);
      const t = byId.get(r.id);
      if (!t) return;
      if (!r.ok || r.ms == null || r.ms < 0) {
        applyLatencyToRow(t.row, -1, true);
        fail++;
        if (r.error && r.error !== 'aborted' && r.error !== 'test aborted' && fail <= 5) {
          log('TEST', 'warn', `[${t.id}] ${r.error}`);
        }
      } else {
        applyLatencyToRow(t.row, r.ms, false);
        ok++;
      }
      done++;
      // Throne polls ~200ms and refreshes list; we log progress every ~25 finishes
      if (done - lastProgressLog >= 25 || done === total) {
        lastProgressLog = done;
        log('TEST', 'info', `${label} ${done}/${total} · ok ${ok} · fail ${fail}`);
      }
    };
    try {
      // listen before invoke so early results paint immediately
      if (urlTestUnlisten) { try { urlTestUnlisten(); } catch (_) {} urlTestUnlisten = null; }
      const un = await tauriListen('net-probe-result', onResult);
      if (typeof un === 'function') {
        urlTestUnlisten = un;
      } else {
        log('TEST', 'warn', 'progressive listen unavailable — will paint when batch ends');
      }
      const payload = targets.map(t => ({ id: t.id, host: t.host, port: t.port }));
      let res;
      try {
        res = await tauriInvoke('net_tcp_probe', {
          targets: payload,
          timeoutMs: 3000,
          concurrency: conc,
        });
      } catch (e) {
        log('TEST', 'warn', t('log.probeUnavailable', { error: e && e.message || e }));
        for (const t of targets) {
          if (!seen.has(t.id)) { applyLatencyToRow(t.row, -1, true); fail++; }
        }
        return;
      }
      // fill any gaps if event stream missed (e.g. no listen API)
      const results = res?.results || [];
      for (const r of results) onResult(r);
      for (const t of targets) {
        if (!seen.has(t.id)) { applyLatencyToRow(t.row, -1, true); fail++; done++; }
      }
      flushLatencyResort();
      const aborted = !!(res?.aborted || urlTestAbort);
      log('OK', 'ok', t('log.testDone', { label, ok, fail, extra: aborted ? t('log.testStopped') : '' }));
      log('SYS', 'info', t('log.testNote'));
      if (typeof saveCatalog === 'function') saveCatalog(true);
    } finally {
      if (urlTestUnlisten) { try { urlTestUnlisten(); } catch (_) {} urlTestUnlisten = null; }
      urlTesting = false;
    }
  }
  let resolveIpRunning = false;
  function applyResolvedIpToRow(row, t, primary, allIps) {
    if (!row || !t) return false;
    const addrEl = row.querySelector('.addr');
    // already IP / IPv6 — keep, still show log
    const hostIsIp = /^\d+\.\d+\.\d+\.\d+$/.test(t.host) || (t.host.includes(':') && !t.host.includes('.'));
    if (hostIsIp) {
      if (addrEl) addrEl.title = (allIps || [primary]).join(', ');
      return false;
    }
    const newAddr = primary + ':' + t.port;
    if (addrEl) {
      addrEl.textContent = newAddr;
      addrEl.title = (allIps || [primary]).join(', ');
      // brief flash so progressive update is visible
      addrEl.style.transition = 'color .15s';
      addrEl.style.color = 'var(--accent, #0a84ff)';
      setTimeout(() => { addrEl.style.color = ''; }, 400);
    }
    const gid = typeof currentGid === 'function' ? currentGid() : 'default';
    const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
    const node = prof?.nodes?.find(x => x.name === t.id);
    if (node) node.addr = newAddr;
    return true;
  }
  async function resolveSelectedIps() {
    // Same progressive model as URL test: ONE Rust batch + net-resolve-result events.
    // Per-row net_resolve_host IPC + per-row log freezes the webview on "select all".
    if (resolveIpRunning) { log('TEST', 'warn', t('log.exitIpBusy')); return; }
    const rows = selectedRows();
    if (!rows.length) { log('SYS', 'warn', t('log.noNode')); return; }
    const byId = new Map();
    const targets = [];
    for (const row of rows) {
      const t = parseRowTarget(row);
      if (!t) continue;
      const addrEl = row.querySelector('.addr');
      if (addrEl) addrEl.style.opacity = '0.55';
      byId.set(t.id, { row, t });
      targets.push({ id: t.id, host: t.host });
    }
    if (!targets.length) { log('SYS', 'warn', t('log.noValidAddr')); return; }
    resolveIpRunning = true;
    log('TEST', 'info', t('log.exitIpStart', { n: targets.length }));
    let replaced = 0, fail = 0, done = 0;
    const seen = new Set();
    // throttle log spam: only summary + first few failures
    let failLogged = 0;
    let unlisten = null;
    const onResult = (payload) => {
      const r = payload?.payload ?? payload;
      if (!r || !r.id) return;
      if (seen.has(r.id)) return;
      seen.add(r.id);
      const item = byId.get(r.id);
      if (!item) return;
      const { row, t } = item;
      try {
        if (r.ok && r.ips && r.ips.length) {
          if (applyResolvedIpToRow(row, t, r.ips[0], r.ips)) replaced++;
        } else {
          fail++;
          if (failLogged < 3) {
            failLogged++;
            log('SYS', 'warn', t('log.exitIpFail', { id: t.id || t.host, error: r.error || t('log.noAddr') }));
          }
        }
      } finally {
        const addrEl = row.querySelector('.addr');
        if (addrEl) addrEl.style.opacity = '';
        done++;
      }
    };
    try {
      if (typeof tauriListen === 'function') {
        unlisten = await tauriListen('net-resolve-result', onResult);
      }
      let res = null;
      try {
        res = await tauriInvoke('net_resolve_hosts', {
          targets,
          concurrency: Math.min(16, Math.max(4, targets.length)),
        });
      } catch (e) {
        // fallback: old single-host path only if batch cmd missing
        log('TEST', 'warn', t('log.exitIpFallback'));
        for (const { id, host } of targets) {
          try {
            const one = await tauriInvoke('net_resolve_host', { host });
            onResult({ id, host, ok: !!(one?.ips?.length), ips: one?.ips || [], error: null });
          } catch (err) {
            onResult({ id, host, ok: false, ips: [], error: String(err && err.message || err) });
          }
        }
        res = { results: [] };
      }
      // fill gaps if events missed
      const results = res?.results || [];
      for (const r of results) onResult(r);
      for (const [id, item] of byId) {
        if (!seen.has(id)) {
          const addrEl = item.row.querySelector('.addr');
          if (addrEl) addrEl.style.opacity = '';
          fail++;
          done++;
        }
      }
      log('SYS', 'ok', t('log.exitIpDone', { replaced, fail, done }));
    } finally {
      if (unlisten) { try { unlisten(); } catch (_) {} }
      resolveIpRunning = false;
    }
  }
  function stopUrlTest() {
    if (!urlTesting) { log('TEST', 'info', t('log.noTestJob')); return; }
    urlTestAbort = true;
    // stop both paths; whichever is live will honor
    tauriInvoke('net_tcp_probe_stop').catch(() => {});
    tauriInvoke('core_url_test_stop').catch(() => {});
    log('TEST', 'warn', t('log.stoppingTest'));
  }
  document.querySelectorAll('#testMenu [data-test]').forEach(btn => {
    btn.addEventListener('click', () => {
      closeMenus();
      const act = btn.dataset.test;
      if (act === 'url-selected') runUrlTest('selected');
      else if (act === 'url-group') runUrlTest('group');
      else if (act === 'stop') stopUrlTest();
      else if (act === 'clear') {
        clearTestResults();
      }
    });
  });
  function clearTestResults() {
    // on_menu_clear_test_result: ClearTestResults for current group
    nodeTable.querySelectorAll('tr .lat').forEach(el => { el.textContent = '—'; el.className = 'lat muted'; });
    nodeTable.querySelectorAll('tr').forEach(r => { r.dataset.lat = '—'; });
    const gid = typeof currentGid === 'function' ? currentGid() : 'default';
    const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
    if (prof?.nodes) prof.nodes.forEach(n => { n.lat = null; });
    log('SYS', 'info', t('log.clearedTests'));
  }
  // on_menu_update_subscription → AsyncUpdate(group->url, group->id)
  let subUpdating = false;
  function ensureProfile(gid, label, foot) {
    let bag;
    try { bag = SUB_PROFILES; } catch (_) { return null; }
    if (!bag) return null;
    if (!bag[gid]) {
      bag[gid] = { label: label || gid, foot: foot || (gid + '.local'), nodes: [] };
    }
    return bag[gid];
  }

  /** iGG/Quantumult-style transport: obfs → network, obfsParam → Host header. */
  function mapObfsNetwork(obfs) {
    const o = String(obfs || '').toLowerCase();
    if (!o || o === 'none' || o === 'plain' || o === 'tcp' || o === 'raw') return '';
    if (o === 'websocket' || o === 'ws') return 'ws';
    if (o === 'grpc') return 'grpc';
    if (o === 'http' || o === 'h2' || o === 'httpupgrade') return o === 'h2' ? 'http' : o;
    if (o === 'quic') return 'quic';
    return o;
  }
  /** sing-box full config outbounds: skip groups / non-proxy. */
  const SINGBOX_SKIP_TYPES = new Set([
    'selector', 'urltest', 'direct', 'block', 'dns', 'pass', 'compatible', 'relay', 'loadbalance',
  ]);
  function isSingBoxProxyOutbound(o) {
    if (!o || typeof o !== 'object') return false;
    const t = String(o.type || '').toLowerCase();
    if (!t || SINGBOX_SKIP_TYPES.has(t)) return false;
    if (!o.server && t !== 'wireguard') return false;
    return [
      'vmess', 'vless', 'trojan', 'http', 'socks', 'shadowsocks',
      'hysteria', 'hysteria2', 'tuic', 'anytls', 'wireguard', 'ssh',
    ].includes(t);
  }
  /** Prefer feed outbound as-is (iGG platform/singbox). Strip junk, keep transport/tls truth. */
  function sanitizeSingBoxOutbound(o) {
    let ob;
    try { ob = JSON.parse(JSON.stringify(o)); } catch (_) { return null; }
    if (!ob || typeof ob !== 'object') return null;
    ob.tag = 'proxy';
    // iGG http path is often {} — not a valid path string
    if (ob.path != null && typeof ob.path !== 'string') delete ob.path;
    if (ob.headers && typeof ob.headers === 'object' && !Object.keys(ob.headers).length) delete ob.headers;
    // tls.enabled:false → omit (plain WS VMess); keep true with server_name
    if (ob.tls && typeof ob.tls === 'object') {
      if (ob.tls.enabled === false || ob.tls.enabled === 'false' || ob.tls.enabled === 0) {
        delete ob.tls;
      } else {
        ob.tls.enabled = true;
      }
    }
    // network "tcp" + transport.ws is normal; network alone is not transport
    if (ob.network === 'tcp' || ob.network === 'raw') delete ob.network;
    return ob;
  }
  /** Clash/JSON/iGG field bag → sing-box outbound (upstream Build subset). No invented secrets. */
  function buildOutboundFromFields(f) {
    const typeRaw = String(f.type || '').toLowerCase();
    const server = String(f.server || '').trim();
    const port = Number(f.port || 443) || 443;
    if (!server) return null;
    const mapType = (tt) => {
      if (tt === 'ss' || tt === 'shadowsocks') return 'shadowsocks';
      if (tt === 'socks' || tt === 'socks5' || tt === 'ssocks5') return 'socks';
      // http.cpp Build: type always "http"; HTTPS = scheme/tls.enabled
      if (tt === 'http' || tt === 'https') return 'http';
      if (tt === 'vmess') return 'vmess';
      if (tt === 'vless') return 'vless';
      if (tt === 'trojan') return 'trojan';
      if (tt === 'anytls') return 'anytls';
      // tuic.cpp Build: type tuic; DisplayType TUIC
      if (tt === 'tuic' || tt === 'tuic5') return 'tuic';
      return tt || '';
    };
    const ty = mapType(typeRaw);
    if (!ty) return null;
    const ob = { type: ty, tag: 'proxy', server, server_port: port };
    const user = f.username || f.user || '';
    const pass = f.password || f.passwd || '';
    const uuid = f.uuid || f.id || '';
    if (ty === 'http' || ty === 'socks') {
      if (user) ob.username = user;
      if (pass) ob.password = pass;
      // path on http outbound; https scheme / type HTTPS / tls flag → tls.enabled
      if (ty === 'http') {
        // iGG may send path:{} — only real strings
        if (typeof f.path === 'string' && f.path) ob.path = f.path;
        const wantTls = f.tls === true || typeRaw === 'https' || !!f.sni;
        if (wantTls) {
          ob.tls = { enabled: true };
          if (f.sni) ob.tls.server_name = f.sni;
          if (f.skip) ob.tls.insecure = true;
        }
      }
    } else if (ty === 'shadowsocks') {
      if (!pass) return null;
      ob.method = f.cipher || f.method || 'aes-128-gcm';
      ob.password = pass;
    } else if (ty === 'vmess') {
      if (!uuid) return null;
      ob.uuid = uuid;
      // cipher only (auto/aes-128-gcm/chacha20-poly1305) — NOT TLS
      const sec = f.cipher || f.security || f.method || 'auto';
      if (sec && sec !== 'auto' && sec !== 'none' && sec !== 'tls') ob.security = sec;
      const aid = Number(f.alterId ?? f.alter_id ?? f.aid ?? 0) || 0;
      if (aid > 0) ob.alter_id = aid;
      // iGG plain WS: no TLS. Only when tls flag true (or explicit sni + tls not false).
      const tlsOn = f.tls === true || f.tls === 'tls' || f.tls === 'true';
      if (tlsOn) {
        ob.tls = { enabled: true };
        if (f.sni) ob.tls.server_name = f.sni;
        if (f.skip) ob.tls.insecure = true;
      }
      const net = String(f.network || f.net || 'tcp').toLowerCase();
      if (net && net !== 'tcp' && net !== 'raw' && net !== 'none') {
        const tr = { type: (net === 'websocket' || net === 'ws') ? 'ws' : net };
        if (f.path) tr.path = f.path;
        if (f.host) tr.headers = { Host: f.host };
        ob.transport = tr;
      }
    } else if (ty === 'vless') {
      if (!uuid) return null;
      ob.uuid = uuid;
      if (f.flow) ob.flow = f.flow;
      // explicit false (edit checkbox off) must not inject TLS; missing/true → on (common VLESS)
      if (f.tls !== false) {
        ob.tls = { enabled: true };
        if (f.sni) ob.tls.server_name = f.sni;
        if (f.skip) ob.tls.insecure = true;
      }
      const net = String(f.network || f.net || 'tcp').toLowerCase();
      if (net && net !== 'tcp' && net !== 'raw' && net !== 'none') {
        const tr = { type: (net === 'websocket' || net === 'ws') ? 'ws' : net };
        if (f.path) tr.path = f.path;
        if (f.host) tr.headers = { Host: f.host };
        ob.transport = tr;
      }
    } else if (ty === 'trojan') {
      if (!pass) return null;
      ob.password = pass;
      ob.tls = { enabled: true };
      if (f.sni) ob.tls.server_name = f.sni;
      if (f.skip) ob.tls.insecure = true;
    } else if (ty === 'anytls') {
      // anyTLS::Build — password + tls always on
      if (!pass) return null;
      ob.password = pass;
      ob.tls = { enabled: true };
      if (f.sni) ob.tls.server_name = f.sni;
      if (f.skip) ob.tls.insecure = true;
    } else if (ty === 'tuic') {
      // tuic::Build — uuid + password + congestion_control; tls always on
      if (!uuid) return null;
      ob.uuid = uuid;
      if (pass) ob.password = pass;
      // iGG: proto "bbr" → congestion_control; Clash: congestion_controller
      const cc = String(f.congestion_control || f.congestion || f.proto || '').trim();
      if (cc && cc !== 'none') ob.congestion_control = cc.toLowerCase();
      if (f.udp_relay_mode) ob.udp_relay_mode = String(f.udp_relay_mode);
      ob.tls = { enabled: true };
      if (f.sni) ob.tls.server_name = f.sni;
      if (f.skip) ob.tls.insecure = true;
      // iGG alpn "h3" or "h3,h2" → tls.alpn array
      const alpnRaw = f.alpn || '';
      if (alpnRaw) {
        const list = String(alpnRaw).split(/[,\s]+/).map(s => s.trim()).filter(Boolean);
        if (list.length) ob.tls.alpn = list;
      }
    } else {
      return null;
    }
    return ob;
  }
  function nodeFromProxyObject(o, i) {
    if (!o || typeof o !== 'object') return null;
    // skip sing-box groups / non-dial outbounds
    const typeProbe = String(o.type || o.protocol || '').toLowerCase();
    if (SINGBOX_SKIP_TYPES.has(typeProbe)) return null;
    // iGG / Quantumult-style: host+title+user; Clash: server+name+username
    const server = o.server || o.address || o.host || o.hostname || o.add || '';
    const port = o.server_port || o.port || 443;
    const name = o.name || o.title || o.tag || o.ps || t('js.nodeN', { n: i + 1 });
    const typeRaw = o.type || o.protocol || 'VLESS';
    const type = normalizeNodeType(typeRaw);
    if (!server) return null;
    // iGG platform/singbox: object is already a dial outbound — keep transport/tls/security as-is
    if (isSingBoxProxyOutbound(o) && (o.server_port != null || o.uuid || o.password || o.username || o.transport)) {
      const outbound = sanitizeSingBoxOutbound(o);
      if (outbound) {
        const node = {
          name,
          type: normalizeNodeType(outbound.type || typeRaw),
          addr: String(outbound.server || server) + ':' + String(outbound.server_port || port),
          lat: null,
          flow: null,
          outbound,
        };
        // HTTPS pill when http + tls
        if (outbound.type === 'http' && outbound.tls && outbound.tls.enabled) node.type = 'HTTPS';
        return node;
      }
    }
    const isHttps = String(typeRaw).toUpperCase() === 'HTTPS' || type === 'HTTPS';
    const isAnytls = type === 'AnyTLS';
    const isTuic = type === 'TUIC';
    // transport object (sing-box) OR Clash ws-opts OR iGG obfsParam
    const tr = (o.transport && typeof o.transport === 'object') ? o.transport : null;
    const hdrHost = (tr && tr.headers && (tr.headers.Host || tr.headers.host))
      || (typeof o['ws-opts'] === 'object' && o['ws-opts'] && o['ws-opts'].headers && o['ws-opts'].headers.Host)
      || (o.headers && (o.headers.Host || o.headers.host))
      || o.obfsParam || o['obfs-param'] || o.obfs_param
      || (tr && tr.host)
      || '';
    // Prefer transport.type (ws) over network:"tcp" (iGG singbox sets both)
    const netFromObfs = mapObfsNetwork(o.obfs)
      || mapObfsNetwork(tr && tr.type)
      || mapObfsNetwork(o.network || o.net || '');
    // iGG alpn string | Clash alpn array | tls.alpn
    let alpn = '';
    if (Array.isArray(o.alpn)) alpn = o.alpn.join(',');
    else if (typeof o.alpn === 'string') alpn = o.alpn;
    else if (o.tls && typeof o.tls === 'object' && Array.isArray(o.tls.alpn)) alpn = o.tls.alpn.join(',');
    // iGG VMess: method=auto is cipher; password often is sub token — not VMess auth
    const isVmess = type === 'VMess' || String(typeRaw).toLowerCase() === 'vmess';
    // tls object with enabled:false must NOT become tls on
    const tlsObjOff = o.tls && typeof o.tls === 'object' && (o.tls.enabled === false || o.tls.enabled === 'false' || o.tls.enabled === 0);
    const tlsExplicit = !tlsObjOff && (
      o.tls === true || o.tls === 'tls' || o.tls === 'true'
      || (o.tls && typeof o.tls === 'object' && o.tls.enabled === true)
    );
    // VMess plain WS (iGG): do NOT infer TLS from empty peer/sni or tls.server_name when enabled:false
    const tlsOn = isHttps || isAnytls || isTuic || tlsExplicit
      || (!isVmess && !tlsObjOff && !!(o.peer || o.sni));
    const pathRaw = (tr && tr.path) || o.path || (o['ws-opts'] && o['ws-opts'].path) || '';
    const path = (typeof pathRaw === 'string') ? pathRaw : '';
    const fields = {
      type: typeRaw,
      server,
      port,
      username: o.username || o.user || '',
      password: isVmess ? '' : (o.password || o.passwd || ''),
      uuid: o.uuid || o.id || '',
      // VMess cipher: prefer scy/cipher; method "auto" ok; never use TLS word
      cipher: isVmess
        ? (o.cipher || o.scy || o.security || (o.method && o.method !== 'none' ? o.method : '') || 'auto')
        : (o.cipher || o.method || o.security || o.scy || ''),
      method: o.method || o.cipher || '',
      alterId: o.alterId ?? o.alter_id ?? o.aid ?? 0,
      tls: tlsOn,
      sni: tlsOn
        ? (o.sni || o.peer || o.servername || o.server_name
          || (o.tls && typeof o.tls === 'object' && o.tls.server_name) || '')
        : '',
      skip: !!(o['skip-cert-verify'] || o.insecure || o.allowInsecure || o.allow_insecure
        || (o.tls && typeof o.tls === 'object' && o.tls.insecure)),
      network: netFromObfs,
      path,
      host: (typeof hdrHost === 'string' ? hdrHost : '') || '',
      flow: o.flow || '',
      // iGG TUIC: proto=bbr; Clash: congestion-controller / congestion_control
      // VMess: proto "none" is not congestion
      congestion_control: isVmess ? '' : (o.congestion_control || o['congestion-controller'] || o.congestion_controller
        || o.congestion || o.proto || ''),
      alpn,
      udp_relay_mode: o.udp_relay_mode || o['udp-relay-mode'] || '',
    };
    if (o.tls === true) fields.tls = true;
    // http/socks: password-only → username = password
    if (!fields.username && fields.password && (type === 'HTTP' || type === 'HTTPS' || type === 'SOCKS')) {
      fields.username = fields.password;
    }
    // AnyTLS: password is auth; some feeds put token only in password
    if (isAnytls && !fields.password && fields.uuid) {
      fields.password = fields.uuid;
    }
    const outbound = buildOutboundFromFields(fields);
    const link = (typeof o.share === 'string' && o.share) || (typeof o.link === 'string' && o.link) || (typeof o.uri === 'string' && o.uri) || null;
    const node = { name, type, addr: server + ':' + port, lat: null, flow: null };
    if (outbound) node.outbound = outbound;
    if (link) node.link = link;
    return node;
  }
  function pickClashField(block, k) {
    const re = new RegExp('(?:^|[\\n,])\\s*' + k + '\\s*:\\s*(?:"([^"]*)"|' + "'" + "([^']*)'" + '|([^,\\n}]+))', 'i');
    const mm = String(block).match(re);
    return mm ? (mm[1] ?? mm[2] ?? mm[3] ?? '').trim() : '';
  }
  function clashBlockToNode(block, i) {
    const name = pickClashField(block, 'name');
    const type = pickClashField(block, 'type') || 'ss';
    const server = pickClashField(block, 'server');
    const port = pickClashField(block, 'port') || '443';
    if (!server || !name) return null;
    const tlsRaw = pickClashField(block, 'tls');
    const fields = {
      type,
      server,
      port,
      username: pickClashField(block, 'username') || pickClashField(block, 'user'),
      password: pickClashField(block, 'password') || pickClashField(block, 'passwd'),
      uuid: pickClashField(block, 'uuid') || pickClashField(block, 'id'),
      cipher: pickClashField(block, 'cipher') || pickClashField(block, 'method'),
      alterId: pickClashField(block, 'alterId') || pickClashField(block, 'alter-id') || '0',
      tls: /^(true|tls|1)$/i.test(tlsRaw),
      sni: pickClashField(block, 'sni') || pickClashField(block, 'servername'),
      skip: /^(true|1)$/i.test(pickClashField(block, 'skip-cert-verify') || pickClashField(block, 'skip_cert_verify')),
      network: pickClashField(block, 'network') || pickClashField(block, 'net'),
      path: pickClashField(block, 'path'),
      host: pickClashField(block, 'host'),
      flow: pickClashField(block, 'flow'),
    };
    const outbound = buildOutboundFromFields(fields);
    const node = { name, type: normalizeNodeType(type), addr: server + ':' + port, lat: null, flow: null };
    if (outbound) node.outbound = outbound;
    return node;
  }

  /** Display meta for one share URI. Authority-only host (query @ must not win). */
  function shareDisplayMeta(line, idx) {
    const raw = String(line || '').trim();
    const m = raw.match(/^(vless|trojan|ss|vmess|hysteria2?|hy2|socks5?|tuic|anytls|https?):\/\//i);
    if (!m) return null;
    if (/^https?:\/\//i.test(raw) && !raw.includes('@')) return null; // sub URL, not proxy share
    let type = m[1].toUpperCase()
      .replace('HY2', 'Hysteria2').replace('HYSTERIA2', 'Hysteria2')
      .replace('SOCKS5', 'SOCKS');
    if (type === 'HTTPS') type = 'HTTPS';
    else if (type === 'HTTP') type = 'HTTP';

    let name = '';
    const hash = raw.indexOf('#');
    if (hash >= 0) {
      try { name = decodeURIComponent(raw.slice(hash + 1)); } catch (_) { name = raw.slice(hash + 1); }
    }

    // v2rayN vmess://base64(json) — body has no user@host
    if (/^vmess:\/\//i.test(raw)) {
      const rest = raw.replace(/^vmess:\/\//i, '').split('#')[0].trim();
      if (rest && !rest.includes('@')) {
        const decoded = tryB64Decode(rest);
        if (decoded) {
          try {
            const j = JSON.parse(String(decoded).trim());
            if (j && typeof j === 'object') {
              const host = String(j.add || j.host || j.server || '').trim();
              const port = j.port != null && String(j.port) !== '' ? String(j.port) : '443';
              if (host) {
                if (!name) name = String(j.ps || j.name || j.remark || '').trim();
                if (!name) name = 'VMess-' + ((idx || 0) + 1);
                return {
                  name,
                  type: normalizeNodeType('vmess'),
                  addr: host.includes(':') ? host : (host + ':' + port),
                };
              }
            }
          } catch (_) { /* fall through to URI parse */ }
        }
      }
    }

    // scheme://[userinfo@]host[:port][/path][?query][#frag] — strip query before @ split
    // (free lists put Telegram=@foo in query; lastIndexOf('@') on full body ate real host)
    let addr = '—';
    try {
      const withoutScheme = raw.replace(/^[a-z0-9+.-]+:\/\//i, '');
      const withoutFrag = withoutScheme.split('#')[0];
      const authorityPath = withoutFrag.split('?')[0];
      const authority = authorityPath.split('/')[0] || '';
      let hostport = authority;
      const at = authority.lastIndexOf('@');
      if (at >= 0) hostport = authority.slice(at + 1);
      hostport = String(hostport || '').trim();
      if (hostport) {
        if (hostport.includes(':')) addr = hostport;
        else addr = hostport + (type === 'HTTP' ? ':80' : ':443');
      }
    } catch (_) {}

    if (!name) name = type + '-' + ((idx || 0) + 1);
    return { name, type: normalizeNodeType(type), addr };
  }
  /** Display-only share lines (no outbound). Desktop import uses Rust sub_parse_share. */
  function mixedPort() {
    return Number(window.__NEXUS_MIXED_PORT__) || 2080;
  }
  function parseShareLinesDisplay(text) {
    const out = [];
    const lines = String(text || '').split(/[\r\n]+/).map(s => s.trim()).filter(Boolean);
    for (const line of lines) {
      const meta = shareDisplayMeta(line, out.length);
      if (!meta) continue;
      out.push({
        name: meta.name,
        type: meta.type,
        addr: meta.addr,
        lat: null,
        flow: null,
        link: line,
      });
    }
    return out;
  }
  /** True when stored display addr is garbage from old lastIndexOf('@') / raw vmess b64. */
  function addrLooksBroken(addr) {
    const a = String(addr || '').trim();
    if (!a || a === '—') return true;
    if (a.includes('&')) return true; // query tail mistaken for host
    if (a.length > 80) return true;
    // bare base64 blob (vmess body used as host)
    if (/^[A-Za-z0-9+/_=-]{40,}$/.test(a) && !a.includes(':')) return true;
    if (/^eyJ/i.test(a)) return true;
    return false;
  }
  function healNodeDisplay(n) {
    if (!n || typeof n !== 'object') return n;
    const link = n.link;
    if (!link || typeof link !== 'string') return n;
    const name = String(n.name || '');
    const synthetic = !name || /^(VLESS|Trojan|VMess|VMESS|SS|Hysteria2|TUIC|AnyTLS|SOCKS|HTTP|HTTPS)-\d+$/i.test(name);
    const broken = addrLooksBroken(n.addr);
    if (!broken && !synthetic) return n;
    const meta = shareDisplayMeta(link, 0);
    if (!meta) return n;
    if (broken) n.addr = meta.addr;
    if (synthetic && meta.name) n.name = meta.name;
    if (meta.type) n.type = meta.type;
    return n;
  }
  function tryB64Decode(text) {
    const s = String(text || '').replace(/\s+/g, '');
    if (s.length < 16 || s.length % 4 === 1) return null;
    if (!/^[A-Za-z0-9+\/_=-]+$/.test(s)) return null;
    try {
      const norm = s.replace(/-/g, '+').replace(/_/g, '/');
      const pad = norm + '===='.slice((norm.length % 4) || 4);
      const bin = atob(pad);
      // reject binary noise
      if (/[\x00-\x08\x0e-\x1f]/.test(bin.slice(0, 200))) return null;
      return bin;
    } catch (_) { return null; }
  }
  function normalizeNodeType(t) {
    const u = String(t || 'VLESS').toUpperCase().replace(/[^A-Z0-9]/g, '');
    if (u === 'VLESS') return 'VLESS';
    if (u === 'TROJAN') return 'Trojan';
    if (u === 'VMESS') return 'VMess';
    if (u === 'SS' || u === 'SHADOWSOCKS') return 'SS';
    if (u === 'HY2' || u === 'HYSTERIA2' || u === 'HYSTERIA') return 'Hysteria2';
    if (u === 'TUIC') return 'TUIC';
    // iGG: SSocks5 / Socks5 / SOCKS5
    if (u === 'SOCKS' || u === 'SOCKS5' || u === 'SSOCKS5') return 'SOCKS';
    if (u === 'ANYTLS') return 'AnyTLS';
    // UI pill: HTTPS vs HTTP (sing-box outbound type stays "http"+tls)
    if (u === 'HTTPS') return 'HTTPS';
    if (u === 'HTTP') return 'HTTP';
    return t || 'VLESS';
  }
  // Sync incomplete share parse removed (4A): all import paths use parseSubscriptionBodyAsync.
  /** Map Rust share/clash node bag → catalog node (keeps outbound + link). */
  function nodeFromRustSub(n) {
    if (!n || typeof n !== 'object') return null;
    const node = {
      name: n.name || 'node',
      type: normalizeNodeType(n.type || 'VLESS'),
      addr: n.addr || '—',
      lat: null,
      flow: null,
    };
    if (typeof n.link === 'string' && n.link) node.link = n.link;
    if (n.outbound && typeof n.outbound === 'object') node.outbound = n.outbound;
    if (node.outbound && node.outbound.type === 'http' && node.outbound.tls && node.outbound.tls.enabled) {
      node.type = 'HTTPS';
    }
    return node;
  }
  /** Throne-style body parse: share/clash always via Rust when desktop; JSON stays JS. */
  async function parseSubscriptionBodyAsync(raw) {
    let text = String(raw || '').trim();
    if (!text) return [];
    if (/^<!DOCTYPE\s+html/i.test(text) || /^<html[\s>]/i.test(text)) return [];
    const inv = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
    const hasRust = typeof inv === 'function';
    /** Say which protocols were dropped. "Imported 0 nodes" alone cannot tell an
     *  empty subscription apart from one we have no parser for. */
    const noteSkipped = (res) => {
      const list = res?.skipped;
      if (Array.isArray(list) && list.length) {
        log('SYS', 'warn', t('log.importSkipped', { list: list.join(', ') }));
      }
    };
    // Prefer Rust share parse whenever invoke exists (not only when line looks like URI).
    if (hasRust) {
      try {
        const res = await inv('sub_parse_share', { body: text });
        noteSkipped(res);
        if (res && res.ok && Array.isArray(res.nodes) && res.nodes.length) {
          return res.nodes.map(nodeFromRustSub).filter(Boolean);
        }
      } catch (e) {
        console.warn('sub_parse_share', e);
      }
    }
    let nodes = hasRust ? [] : parseShareLinesDisplay(text);
    if (nodes.length) return nodes;
    const decoded = tryB64Decode(text);
    if (decoded) {
      if (hasRust) {
        try {
          const res = await inv('sub_parse_share', { body: decoded });
          noteSkipped(res);
          if (res && res.ok && Array.isArray(res.nodes) && res.nodes.length) {
            return res.nodes.map(nodeFromRustSub).filter(Boolean);
          }
        } catch (e) {
          console.warn('sub_parse_share b64', e);
        }
      } else {
        nodes = parseShareLinesDisplay(decoded);
        if (nodes.length) return nodes;
      }
      text = decoded.trim();
    }
    try {
      const j = JSON.parse(text);
      const arr = Array.isArray(j) ? j : (j.proxies || j.outbounds || j.nodes || []);
      if (Array.isArray(arr) && arr.length) {
        nodes = arr.map((o, i) => nodeFromProxyObject(o, i)).filter(Boolean);
        if (nodes.length) return nodes;
      }
    } catch (_) {}
    // Clash: real YAML in Rust (Throne fkYAML path) — nested ws-opts/reality-opts/…
    if (/\bproxies\s*:/i.test(text)) {
      if (typeof inv === 'function') {
        try {
          const res = await inv('sub_parse_clash', { body: text });
          noteSkipped(res);
          if (res && res.ok && Array.isArray(res.nodes) && res.nodes.length) {
            return res.nodes.map(nodeFromRustSub).filter(Boolean);
          }
        } catch (e) {
          console.warn('sub_parse_clash', e);
        }
      }
      // last-resort flat heuristic if Rust unavailable (dev browser)
      const out = [];
      const re = /\{([^{}]*(?:name|server|type|port)[^{}]*)\}/g;
      let m;
      while ((m = re.exec(text)) && out.length < 5000) {
        const n = clashBlockToNode(m[1], out.length);
        if (n) out.push(n);
      }
      if (out.length) return out;
    }
    return [];
  }
  async function fetchSubscriptionBody(url) {
    const u = String(url || '').trim();
    if (!u) throw new Error('empty url');
    const inv = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
    if (typeof inv === 'function') {
      const res = await inv('sub_fetch', { url: u });
      if (res && res.ok && typeof res.body === 'string') return res.body;
      throw new Error((res && res.error) || 'fetch failed');
    }
    // browser / non-tauri fallback
    const r = await fetch(u, { method: 'GET', cache: 'no-store', credentials: 'omit' });
    if (!r.ok) throw new Error('HTTP ' + r.status);
    return await r.text();
  }
  function setGroupUpdateStatus(msg, kind) {
    const el = document.getElementById('groupUpdateStatus');
    if (!el) return;
    el.textContent = msg || '';
    el.dataset.kind = kind || '';
    el.hidden = !msg;
  }
  function updateSubscription(gid, { all = false } = {}) {
    if (subUpdating) {
      log('SYS', 'warn', t('log.subBusy'));
      setGroupUpdateStatus(t('js.subStillBusy'), 'warn');
      return Promise.resolve(false);
    }
    if (typeof GROUPS === 'undefined' || !Array.isArray(GROUPS)) {
      log('SYS', 'warn', t('log.groupsNotReady'));
      return Promise.resolve(false);
    }
    const groups = all
      ? [...GROUPS]
      : [GROUPS.find(g => g.id === (gid || (typeof activeGroupId === 'function' ? activeGroupId() : 'default')))].filter(Boolean);
    if (!groups.length) {
      log('SYS', 'warn', t('log.noGroups'));
      setGroupUpdateStatus(t('log.noGroups'), 'warn');
      return Promise.resolve(false);
    }
    // should_skip_group: empty url
    const targets = groups.filter(g => g.url && String(g.url).trim());
    const skipped = groups.length - targets.length;
    if (!targets.length) {
      log('SYS', 'warn', t('log.noSubUrl'));
      setGroupUpdateStatus(t('js.noSubSkipped'), 'warn');
      if (!all) {
        const s = document.querySelector('.side-item[data-settings="sub"]');
        if (s) s.click();
        else if (typeof showView === 'function') showView('settings', 'sub');
        if (typeof syncSubUrlField === 'function') setTimeout(syncSubUrlField, 50);
      }
      return Promise.resolve(false);
    }
    subUpdating = true;
    const btn = document.getElementById('refreshBtn');
    const label = btn?.querySelector('[data-i18n="tb.refresh"]') || btn;
    const prev = label?.textContent;
    const allBtn = document.getElementById('groupUpdateAll');
    const allPrev = allBtn?.textContent;
    if (btn) btn.disabled = true;
    if (label) label.textContent = t('js.updating');
    if (allBtn && all) { allBtn.disabled = true; allBtn.textContent = t('js.updating'); }
    setGroupUpdateStatus(all
      ? t('js.subUpdatingN', { n: targets.length })
      : t('js.subUpdatingOne', { name: targets[0].name }), 'info');

    return (async () => {
      let totalAdd = 0, totalDel = 0, ok = 0, fail = 0;
      try {
        for (let i = 0; i < targets.length; i++) {
          const g = targets[i];
          log('SYS', 'info', t('log.subUpdating', { name: g.name }));
          if (allBtn && all) allBtn.textContent = t('js.subUpdatingProgress', { i: i + 1, n: targets.length });
          setGroupUpdateStatus(t('js.subUpdatingNamed', { name: g.name, i: i + 1, n: targets.length }), 'info');
          try {
            let bag;
            try { bag = SUB_PROFILES; } catch (_) { bag = null; }
            if (!bag) throw new Error(t('js.nodesNotReady'));
            const footHost = (String(g.url).match(/https?:\/\/([^/]+)/i) || [])[1] || g.id;
            let prof = bag[g.id];
            if (!prof) {
              prof = { label: g.name, foot: footHost, nodes: [] };
              bag[g.id] = prof;
            }
            const before = (prof.nodes || []).length;
            // GroupUpdater: HttpGet then parse; on fail keep existing nodes
            const body = await fetchSubscriptionBody(g.url);
            const next = await parseSubscriptionBodyAsync(body);
            if (!next.length) {
              throw new Error(t('js.subEmptyParse'));
            }
            const beforeNodes = prof.nodes || [];
            const beforeKeys = new Set(beforeNodes.map(n => (n.addr || '') + '|' + (n.name || '')));
            const nextKeys = new Set(next.map(n => (n.addr || '') + '|' + (n.name || '')));
            let addR = 0, delR = 0;
            for (const k of nextKeys) if (!beforeKeys.has(k)) addR++;
            for (const k of beforeKeys) if (!nextKeys.has(k)) delR++;
            // keep per-node cumulative traffic across sub refresh (match name, then addr)
            const byName = new Map();
            const byAddr = new Map();
            for (const o of beforeNodes) {
              if (o && o.name) byName.set(o.name, o);
              if (o && o.addr) byAddr.set(o.addr, o);
            }
            for (const n of next) {
              const prev = byName.get(n.name) || byAddr.get(n.addr);
              if (!prev) continue;
              if (prev.flowUp || prev.flowDown || prev.flow) {
                n.flowUp = Math.max(0, Number(prev.flowUp) || 0);
                n.flowDown = Math.max(0, Number(prev.flowDown) || 0);
                n.flow = prev.flow || null;
              }
            }
            prof.nodes = next;
            prof.label = g.name;
            try {
              const h = new URL(g.url).hostname;
              if (h) prof.foot = h;
            } catch (_) { prof.foot = footHost; }
            g.count = next.length;
            if (typeof saveCatalog === 'function') saveCatalog();
            totalAdd += addR;
            totalDel += delR;
            ok++;
            log('OK', 'ok', t('log.subUpdated', { name: g.name, add: addR, del: delR, total: next.length }));
            if (typeof renderGroupList === 'function') renderGroupList();
          } catch (err) {
            fail++;
            // keep existing nodes — no random replace
            log('SYS', 'warn', t('log.subFailKeep', { name: g.name, error: err && err.message || err, n: g.count || 0 }));
          }
        }
        const cur = typeof activeGroupId === 'function' ? activeGroupId() : 'default';
        if (typeof renderNodes === 'function') renderNodes(cur);
        if (typeof renderGroupList === 'function') renderGroupList();
        if (typeof syncSubUrlField === 'function') syncSubUrlField();
        const skipMsg = skipped ? t('js.subSkipUrl', { n: skipped }) : '';
        if (fail && !ok) {
          setGroupUpdateStatus(t('js.subFailStatus', { skip: skipMsg }), 'warn');
          log('SYS', 'warn', t('log.subFail'));
          return false;
        }
        const failBit = fail ? t('js.subFailN', { n: fail }) : '';
        const summary = t('js.subSummary', { ok, add: totalAdd, del: totalDel, skip: skipMsg, fail: failBit });
        setGroupUpdateStatus(summary, fail ? 'warn' : 'ok');
        if (targets.length > 1 || all) log('SYS', 'ok', t('log.subAllDone', { summary }));
        return true;
      } catch (err) {
        log('SYS', 'warn', t('log.subException', { error: err && err.message || err }));
        setGroupUpdateStatus(t('js.subExceptionStatus', { error: err && err.message || err }), 'warn');
        return false;
      }
    })().finally(() => {
      subUpdating = false;
      if (btn) btn.disabled = false;
      if (label && prev != null) label.textContent = prev || t('tb.refresh');
      if (allBtn) {
        allBtn.disabled = false;
        allBtn.textContent = allPrev || t('groups.updateAll');
      }
    });
  }
  document.getElementById('refreshBtn')?.addEventListener('click', () => {
    updateSubscription(null);
  });
  document.getElementById('clearLog').addEventListener('click', () => {
    logPanel.innerHTML = '';
    log('SYS', 'info', t('log.logCleared'));
  });

  // ——— helpers: clipboard / share / dialogs ———
  function copyText(s) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(s);
    }
    return new Promise((resolve, reject) => {
      try {
        const ta = document.createElement('textarea');
        ta.value = s;
        ta.style.cssText = 'position:fixed;left:-9999px;top:0';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        ta.remove();
        resolve();
      } catch (err) { reject(err); }
    });
  }
  function findNodeByName(name) {
    const gid = typeof currentGid === 'function' ? currentGid() : 'default';
    const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
    return prof?.nodes?.find(n => n.name === name) || null;
  }
  function looksLikeUuid(s) {
    const t = String(s || '').trim();
    if (/^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(t)) return true;
    if (/^[0-9a-fA-F]{32}$/.test(t)) return true;
    return false;
  }
  /** True only for share URIs that can actually Start (not QR decorative fakes). */
  function isShareUri(s) {
    if (!s || typeof s !== 'string') return false;
    if (/^ss:\/\//i.test(s)) return true;
    if (/^(vless|trojan|socks5?|anytls|tuic|https?):\/\//i.test(s) && s.includes('@')) return true;
    if (/^vmess:\/\//i.test(s)) {
      // v2rayN b64 JSON body (no @) OR uuid@host
      const rest = s.replace(/^vmess:\/\//i, '').split('#')[0];
      if (!rest.includes('@')) return rest.length > 16; // likely b64 json
      const user = rest.split('@')[0].split(':')[0];
      return looksLikeUuid(user);
    }
    return false;
  }
  function hasRealShareLink(name) {
    const node = findNodeByName(name);
    if (!node) return false;
    if (node.outbound && node.outbound.type && node.outbound.server) return true;
    return isShareUri(node.link);
  }
  /** Payload for connect_selected: prefer outbound JSON (Clash), else share URI. */
  function nodeConnectPayload(name) {
    const node = findNodeByName(name);
    if (!node) return null;
    if (node.outbound && node.outbound.type && node.outbound.server) {
      // refuse outbound with placeholder uuid (old synthetic)
      if (node.outbound.type === 'vmess' || node.outbound.type === 'vless' || node.outbound.type === 'tuic') {
        const id = node.outbound.uuid || '';
        if (id && !looksLikeUuid(id)) return null;
      }
      // Catalog display IP (右键解析) → PF peer allow when server is hostname/CDN.
      const o = Object.assign({}, node.outbound);
      const m = String(node.addr || '').match(/^(\d{1,3}(?:\.\d{1,3}){3}):\d+$/);
      if (m && !/^\d{1,3}(?:\.\d{1,3}){3}$/.test(String(o.server || ''))) {
        o.server_ip = m[1];
      }
      return { outbound: o };
    }
    if (isShareUri(node.link)) return { link: node.link };
    return null;
  }
  function nodeShareLink(name) {
    const node = findNodeByName(name);
    // Real share only — never invent fake vmess://btoa(name)@host (broke Start + "系统代理不生效")
    if (node?.link && isShareUri(node.link)) return node.link;
    if (node?.outbound && typeof node.outbound === 'object') {
      try { return JSON.stringify(node.outbound); } catch (_) {}
    }
    return '';
  }
  function openDialog(id) {
    const el = document.getElementById(id);
    if (!el) return;
    el.removeAttribute('hidden');
    el.hidden = false;
    el.classList.add('open');
  }
  function closeDialog(id) {
    const el = document.getElementById(id);
    if (!el) return;
    el.classList.remove('open');
    el.hidden = true;
    el.setAttribute('hidden', '');
  }
  /** In-app confirm. Native window.confirm is often a no-op / always-false in Tauri WKWebView. */
  let _askResolve = null;
  function closeAsk(result) {
    const r = _askResolve;
    _askResolve = null;
    closeDialog('askMask');
    const ok = document.getElementById('askOk');
    if (ok) ok.classList.remove('danger');
    if (r) r(!!result);
  }
  function askConfirm(message, opts) {
    opts = opts || {};
    return new Promise((resolve) => {
      // collapse any prior ask
      if (_askResolve) { const prev = _askResolve; _askResolve = null; prev(false); }
      _askResolve = resolve;
      const title = document.getElementById('askTitle');
      const msg = document.getElementById('askMsg');
      const ok = document.getElementById('askOk');
      const card = document.getElementById('askCard');
      if (title) title.textContent = opts.title || t('confirm.askTitle');
      if (msg) msg.textContent = message || '';
      if (ok) {
        ok.textContent = opts.okText || t('btn.ok');
        ok.classList.toggle('danger', !!opts.danger);
      }
      if (card) {
        card.classList.toggle('ask-danger', !!opts.danger);
        card.classList.toggle('warn', !!opts.warn && !opts.danger);
      }
      const cancel = document.getElementById('askCancel');
      if (cancel) cancel.textContent = opts.cancelText || t('btn.cancel');
      openDialog('askMask');
      // focus primary for keyboard
      setTimeout(() => { try { (opts.danger ? cancel : ok)?.focus(); } catch (_) {} }, 0);
    });
  }
  document.getElementById('askOk')?.addEventListener('click', () => closeAsk(true));
  document.getElementById('askCancel')?.addEventListener('click', () => closeAsk(false));
  document.getElementById('askMask')?.addEventListener('click', (e) => {
    if (e.target.id === 'askMask') closeAsk(false);
  });
  /** sing-box outbound → flat edit fields (upstream dialog_edit_profile subset). */
  function fieldsFromOutbound(ob, fallback) {
    const o = (ob && typeof ob === 'object') ? ob : {};
    const ty = String(o.type || fallback?.type || 'vless').toLowerCase();
    const tls = o.tls && typeof o.tls === 'object' ? o.tls : {};
    // UI: http + tls.enabled → HTTPS; anytls → AnyTLS; socks → SOCKS
    let displayType = fallback?.type || ty;
    if (ty === 'http' && (tls.enabled === true || fallback?.type === 'HTTPS')) {
      displayType = 'HTTPS';
    } else if (ty === 'http') {
      displayType = fallback?.type === 'HTTP' ? 'HTTP' : (tls.enabled ? 'HTTPS' : 'HTTP');
    } else if (ty === 'anytls') {
      displayType = 'AnyTLS';
    } else if (ty === 'socks') {
      displayType = 'SOCKS';
    } else if (ty === 'vmess') {
      displayType = 'VMess';
    } else if (ty === 'tuic') {
      displayType = 'TUIC';
    }
    const norm = normalizeNodeType(displayType);
    const server = o.server || (fallback?.addr || '').split(':')[0] || '';
    const port = o.server_port || o.port || (fallback?.addr || '').split(':')[1] || '443';
    const tr = o.transport && typeof o.transport === 'object' ? o.transport : {};
    let hostHdr = '';
    if (tr.headers && typeof tr.headers === 'object') {
      hostHdr = tr.headers.Host || tr.headers.host || '';
    }
    let alpn = '';
    if (Array.isArray(tls.alpn)) alpn = tls.alpn.join(',');
    else if (typeof tls.alpn === 'string') alpn = tls.alpn;
    // throng/sing-box may put WS Host in transport.host (not only headers)
    if (!hostHdr && tr.host) hostHdr = String(tr.host);
    return {
      name: fallback?.name || '',
      type: norm,
      server,
      port: String(port),
      uuid: o.uuid || '',
      flow: o.flow || '',
      security: o.security || 'auto',
      alterId: o.alter_id ?? o.alterId ?? 0,
      username: o.username || '',
      password: o.password || '',
      method: o.method || o.cipher || '',
      // surface SNI whenever present (tls off still keeps server_name in some feeds — show for edit, save gated by TLS checkbox)
      sni: (tls.server_name || o.sni || '') || '',
      network: tr.type || o.network || '',
      path: tr.path || o.path || '',
      host: hostHdr || '',
      insecure: !!(tls.insecure || o.insecure),
      // explicit truth: only enabled===true is ON (missing tls object = off for VMess plain WS)
      tls: tls.enabled === true,
      congestion_control: o.congestion_control || '',
      alpn,
      note: fallback?.note || '',
    };
  }
  function editTypeKey(typeLabel) {
    const u = String(typeLabel || 'VLESS').toUpperCase().replace(/[^A-Z0-9]/g, '');
    if (u === 'VLESS') return 'vless';
    if (u === 'VMESS') return 'vmess';
    if (u === 'TROJAN') return 'trojan';
    if (u === 'SS' || u === 'SHADOWSOCKS') return 'ss';
    if (u === 'HTTPS') return 'https';
    if (u === 'HTTP') return 'http';
    if (u === 'SOCKS' || u === 'SOCKS5' || u === 'SSOCKS5') return 'socks';
    if (u === 'ANYTLS') return 'anytls';
    if (u === 'TUIC' || u === 'TUIC5') return 'tuic';
    return u.toLowerCase();
  }
  function syncEditFieldsByType(typeLabel) {
    const key = editTypeKey(typeLabel);
    document.querySelectorAll('#editForm [data-edit-for]').forEach(el => {
      const keys = String(el.getAttribute('data-edit-for') || '').split(/\s+/).filter(Boolean);
      const show = keys.includes(key);
      el.hidden = !show;
    });
  }
  function setEditField(id, val) {
    const el = document.getElementById(id);
    if (!el) return;
    if (el.type === 'checkbox') el.checked = !!val;
    else el.value = val == null ? '' : String(val);
  }
  function getEditField(id) {
    const el = document.getElementById(id);
    if (!el) return '';
    if (el.type === 'checkbox') return !!el.checked;
    return (el.value || '').trim();
  }
  async function openEditDialog(name) {
    const row = [...nodeTable.querySelectorAll('tr')].find(r => r.dataset.name === name) || selectedRows()[0];
    const n = name || row?.dataset.name || '';
    const node = (typeof findNodeByName === 'function' ? findNodeByName(n) : null) || null;
    const type = node?.type || row?.querySelector('.pill')?.textContent || 'VLESS';
    const addr = node?.addr || row?.querySelector('.addr')?.textContent || '';
    // share-line nodes may only have link — hydrate outbound once via Rust parse_to_outbound
    if (node && !node.outbound && typeof node.link === 'string' && node.link) {
      const inv = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
      if (typeof inv === 'function') {
        try {
          const res = await inv('sub_parse_share', { body: node.link });
          const one = res && res.ok && Array.isArray(res.nodes) ? res.nodes[0] : null;
          if (one && one.outbound && typeof one.outbound === 'object') {
            node.outbound = one.outbound;
            if (one.addr) node.addr = one.addr;
            if (typeof saveCatalog === 'function') saveCatalog(true);
          }
        } catch (e) {
          console.warn('openEditDialog hydrate', e);
        }
      }
    }
    const f = fieldsFromOutbound(node?.outbound, { name: n, type, addr: node?.addr || addr, note: node?.note || '' });
    // share-link only: still show host/port from addr
    if (!node?.outbound && (node?.addr || addr)) {
      const [h, p] = String(node?.addr || addr).split(':');
      if (h) f.server = h;
      if (p) f.port = p;
    }
    setEditField('editName', f.name);
    setEditField('editAddr', f.server);
    setEditField('editPort', f.port);
    setEditField('editUuid', f.uuid);
    setEditField('editFlow', f.flow);
    setEditField('editSecurity', f.security || 'auto');
    setEditField('editAlterId', f.alterId != null ? f.alterId : 0);
    setEditField('editUser', f.username);
    setEditField('editPass', f.password);
    setEditField('editMethod', f.method || 'aes-128-gcm');
    setEditField('editCongest', f.congestion_control || 'bbr');
    setEditField('editAlpn', f.alpn || (editTypeKey(f.type) === 'tuic' ? 'h3' : ''));
    setEditField('editSni', f.sni);
    setEditField('editNetwork', f.network);
    setEditField('editPath', f.path);
    setEditField('editHost', f.host);
    setEditField('editTls', f.tls);
    setEditField('editInsecure', f.insecure);
    setEditField('editNote', f.note);
    const typeBtn = document.getElementById('editType');
    if (typeBtn) {
      typeBtn.dataset.value = f.type;
      const v = typeBtn.querySelector('.sel-val');
      if (v) v.textContent = f.type;
    }
    syncEditFieldsByType(f.type);
    const hint = document.getElementById('editOutboundHint');
    if (hint) {
      const net = String(f.network || '').toLowerCase();
      const missingWs = (net === 'ws' || net === 'websocket') && !f.host;
      const bits = [];
      bits.push(f.tls ? t('js.tlsOn') : t('js.tlsOff'));
      if (f.host) bits.push('Host=' + f.host);
      if (f.path) bits.push('path=' + f.path);
      if (missingWs) bits.push(t('js.wsMissingHost'));
      hint.textContent = bits.join(' · ');
      hint.hidden = false;
    }
    openDialog('editMask');
  }
  document.getElementById('editCancel')?.addEventListener('click', () => closeDialog('editMask'));
  document.getElementById('editMask')?.addEventListener('click', (e) => {
    if (e.target.id === 'editMask') closeDialog('editMask');
  });
  document.getElementById('editSave')?.addEventListener('click', () => {
    const name = getEditField('editName') || t('js.unnamed');
    const host = getEditField('editAddr') || '0.0.0.0';
    const port = getEditField('editPort') || '443';
    const type = document.getElementById('editType')?.dataset.value || 'VLESS';
    const tk = editTypeKey(type);
    const fields = {
      type,
      server: host,
      port,
      uuid: getEditField('editUuid'),
      flow: getEditField('editFlow'),
      security: getEditField('editSecurity'),
      cipher: getEditField('editSecurity') || getEditField('editMethod'),
      method: getEditField('editMethod'),
      alterId: getEditField('editAlterId') || 0,
      username: getEditField('editUser'),
      password: getEditField('editPass'),
      sni: getEditField('editSni'),
      network: getEditField('editNetwork'),
      path: getEditField('editPath'),
      host: getEditField('editHost'),
      skip: getEditField('editInsecure'),
      congestion_control: getEditField('editCongest'),
      alpn: getEditField('editAlpn'),
      // Default TLS only for protocols that always use it; VMess/VLESS/HTTP from checkbox.
      tls: tk === 'https' || tk === 'trojan' || tk === 'tuic' || tk === 'anytls',
    };
    // SS uses method not security; VMess uses security
    if (tk === 'ss') {
      fields.cipher = getEditField('editMethod');
      fields.method = fields.cipher;
    }
    if (tk === 'http') {
      fields.tls = !!getEditField('editTls') || !!getEditField('editSni');
    }
    if (tk === 'https') {
      fields.tls = true;
      fields.type = 'https';
    }
    if (tk === 'anytls') {
      fields.tls = true;
      fields.type = 'anytls';
    }
    if (tk === 'tuic') {
      fields.tls = true;
      fields.type = 'tuic';
    }
    // VMess/VLESS: TLS from checkbox only (iGG plain WS has SNI string but tls.enabled:false)
    if (tk === 'vmess') {
      fields.tls = !!getEditField('editTls');
      fields.type = 'vmess';
      fields.cipher = getEditField('editSecurity') || 'auto';
      fields.security = fields.cipher;
    }
    if (tk === 'vless') {
      fields.tls = !!getEditField('editTls');
      fields.type = 'vless';
    }
    // http/socks: empty user + password → username = password
    if ((tk === 'http' || tk === 'https' || tk === 'socks') && !fields.username && fields.password) {
      fields.username = fields.password;
    }
    let row = selectedRows()[0];
    if (!row) row = [...nodeTable.querySelectorAll('tr')].find(r => r.dataset.name === selectedName);
    const oldName = row?.dataset.name || selectedName;
    if (row) {
      row.dataset.name = name;
      const nameEl = row.querySelector('.name');
      if (nameEl) nameEl.textContent = name;
      const pill = row.querySelector('.pill');
      if (pill) pill.textContent = type;
      const addrEl = row.querySelector('.addr');
      if (addrEl) addrEl.textContent = host + ':' + port;
      selectedName = name;
      // rename of the live tunnel node updates hero label only
      if (connected && connectedName === oldName) {
        connectedName = name;
      }
      if (typeof setConnected === 'function') setConnected(connected, { pin: false, sideEffects: false });
    }
    const gid = typeof currentGid === 'function' ? currentGid() : 'default';
    const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
    if (prof?.nodes) {
      const node = prof.nodes.find(n => n.name === oldName);
      if (node) {
        node.name = name;
        node.type = type;
        node.addr = host + ':' + port;
        node.note = getEditField('editNote') || undefined;
        const outbound = (typeof buildOutboundFromFields === 'function')
          ? buildOutboundFromFields(fields)
          : null;
        if (outbound) {
          // Rebuild from form is source of truth for known protocol keys.
          // Shallow-merge used to keep stale tls/security/password and break iGG VMess.
          if (node.outbound && typeof node.outbound === 'object') {
            const prev = node.outbound;
            const keep = { ...outbound };
            // only carry advanced keys form does not edit
            for (const k of ['multiplex', 'dialer', 'detour', 'domain_strategy', 'domain_resolver', 'bind_interface', 'routing_mark', 'packet_encoding', 'global_padding', 'authenticated_length']) {
              if (prev[k] != null && keep[k] == null) keep[k] = prev[k];
            }
            if (prev.tls && typeof prev.tls === 'object' && keep.tls && typeof keep.tls === 'object') {
              // keep utls/reality extras under tls when still TLS
              for (const k of ['utls', 'reality', 'alpn', 'min_version', 'max_version']) {
                if (prev.tls[k] != null && keep.tls[k] == null) keep.tls[k] = prev.tls[k];
              }
            }
            node.outbound = keep;
          } else {
            node.outbound = outbound;
          }
          // edited outbound supersedes share link (may be stale)
          if (node.link) delete node.link;
        }
      }
    }
    if (typeof saveCatalog === 'function') saveCatalog(true);
    closeDialog('editMask');
    log('SYS', 'ok', t('log.nodeSaved', { name }));
  });

  let _qrCopyPayload = '';
  function setQrBox(html) {
    const box = document.getElementById('qrBox');
    if (box) box.innerHTML = html;
  }
  function formatQrDisplay(raw) {
    if (!raw) return t('qr.noShare');
    const s = String(raw).trim();
    if (s.startsWith('{') || s.startsWith('[')) {
      try { return JSON.stringify(JSON.parse(s), null, 2); } catch (_) { /* keep raw */ }
    }
    return s;
  }
  async function openQrDialog(name) {
    const n = name || selectedName || t('js.nodes');
    const link = nodeShareLink(n);
    _qrCopyPayload = link || '';
    document.getElementById('qrName').textContent = n;
    document.getElementById('qrLink').textContent = formatQrDisplay(link);
    setQrBox('<span class="qr-empty">' + t('js.generating') + '</span>');
    openDialog('qrMask');
    if (!link) {
      setQrBox('<span class="qr-empty">' + t('qr.empty') + '</span>');
      return;
    }
    try {
      const inv = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
      if (typeof inv !== 'function') {
        setQrBox('<span class="qr-empty">' + t('qr.needApp') + '</span>');
        return;
      }
      const res = await inv('qr_svg', { text: link });
      const svg = res && res.svg;
      if (!svg) throw new Error('empty svg');
      setQrBox(svg);
    } catch (e) {
      setQrBox('<span class="qr-empty">' + t('js.qrFail', { err: escHtml(String(e?.message || e)) }) + '</span>');
      log('SYS', 'warn', t('log.qrFail', { error: e?.message || e }));
    }
  }
  document.getElementById('qrClose')?.addEventListener('click', () => closeDialog('qrMask'));
  document.getElementById('qrMask')?.addEventListener('click', (e) => {
    if (e.target.id === 'qrMask') closeDialog('qrMask');
  });
  document.getElementById('qrCopy')?.addEventListener('click', () => {
    const link = _qrCopyPayload || document.getElementById('qrLink').textContent || '';
    if (!link || link === t('qr.noShare')) {
      log('SYS', 'warn', t('log.noLinkCopy'));
      return;
    }
    copyText(link).then(() => log('SYS', 'ok', t('log.linkCopied'))).catch(() => log('SYS', 'info', link));
  });

  // ——— group management ———
  let GROUPS = [
    // empty URL skips update; paste subscription in 设置 → 订阅
    { id: 'default', name: 'Default', count: 0, url: '', autoUpdate: false },
    { id: 'backup', name: '备用', count: 0, url: '', autoUpdate: false },
  ];
  let groupEditId = null;
  let groupEditMode = 'edit'; // edit | create | rename

  // Persist groups + nodes across reload (localStorage). Demo seed only when no snapshot.
  const CATALOG_KEY = 'nexus.catalog.v1';
  let _catalogSaveTimer = null;
  function serializeCatalog() {
    const groups = (GROUPS || []).map(g => ({
      id: g.id,
      name: g.name,
      count: g.count | 0,
      url: g.url || '',
      autoUpdate: !!g.autoUpdate,
    }));
    const profiles = {};
    const bag = (typeof SUB_PROFILES !== 'undefined' && SUB_PROFILES) ? SUB_PROFILES : {};
    for (const id of Object.keys(bag)) {
      const p = bag[id] || {};
      profiles[id] = {
        label: p.label || id,
        foot: p.foot || '',
        nodes: (p.nodes || []).map(n => ({
          name: n.name,
          type: n.type,
          addr: n.addr,
          lat: n.lat == null ? null : n.lat,
          flow: n.flow == null ? null : n.flow,
          flowUp: Math.max(0, Number(n.flowUp) || 0) || undefined,
          flowDown: Math.max(0, Number(n.flowDown) || 0) || undefined,
          // keep connect material
          link: n.link || undefined,
          outbound: n.outbound || undefined,
        })),
      };
    }
    const active = (typeof activeGroupId === 'function')
      ? activeGroupId()
      : (document.querySelector('#subSeg button.active')?.dataset.sub || groups[0]?.id || 'default');
    return { v: 1, active, groups, profiles };
  }
  function saveCatalog(immediate) {
    const run = () => {
      // Serialize at put time so chain always writes latest blob.
      catalogPutChain = catalogPutChain.then(() => flushCatalogPut()).catch(() => {});
    };
    if (immediate) {
      if (_catalogSaveTimer) { clearTimeout(_catalogSaveTimer); _catalogSaveTimer = null; }
      run();
      return;
    }
    if (_catalogSaveTimer) clearTimeout(_catalogSaveTimer);
    _catalogSaveTimer = setTimeout(() => { _catalogSaveTimer = null; run(); }, 80);
  }
  function loadCatalogFromLocal() {
    try {
      const raw = localStorage.getItem(CATALOG_KEY);
      if (!raw) return null;
      const data = JSON.parse(raw);
      if (!data || data.v !== 1 || !Array.isArray(data.groups) || !data.groups.length) return null;
      if (!data.profiles || typeof data.profiles !== 'object') return null;
      return data;
    } catch (_) {
      return null;
    }
  }
  function loadCatalog() {
    // sync path: LS only (boot uses hydrateCatalogOnBoot async store-first)
    return loadCatalogFromLocal();
  }
  function applyCatalog(data) {
    if (!data) return false;
    GROUPS = data.groups.map(g => ({
      id: String(g.id || ''),
      name: String(g.name || g.id || t('tb.groups')),
      count: Number(g.count) || 0,
      url: String(g.url || ''),
      autoUpdate: !!g.autoUpdate,
    })).filter(g => g.id);
    if (!GROUPS.length) return false;
    const next = {};
    for (const g of GROUPS) {
      const p = data.profiles[g.id] || {};
      const nodes = Array.isArray(p.nodes) ? p.nodes.map(n => {
        const o = {
          name: n.name || t('js.nodes'),
          type: n.type || 'VLESS',
          addr: n.addr || '',
          lat: (n.lat == null || n.lat === '') ? null : n.lat,
          flow: n.flow == null ? null : n.flow,
          flowUp: Math.max(0, Number(n.flowUp) || 0),
          flowDown: Math.max(0, Number(n.flowDown) || 0),
        };
        // recover numeric totals from legacy display string if needed
        if ((!o.flowUp && !o.flowDown) && o.flow && typeof o.flow === 'string') {
          const parts = o.flow.split('·').map(s => s.trim());
          const parseOne = (s) => {
            const m = String(s || '').match(/([\d.]+)\s*([KMG]?B)/i);
            if (!m) return 0;
            let v = parseFloat(m[1]);
            const u = m[2].toUpperCase();
            if (u.startsWith('K')) v *= 1024;
            else if (u.startsWith('M')) v *= 1024 * 1024;
            else if (u.startsWith('G')) v *= 1024 * 1024 * 1024;
            return Math.max(0, v) || 0;
          };
          o.flowUp = parseOne(parts[0]);
          o.flowDown = parseOne(parts[1]);
        }
        if (n.link) o.link = n.link;
        if (n.outbound) o.outbound = repairStoredOutbound(n.outbound);
        // old share display used lastIndexOf('@') → query tail as addr (broke table cols)
        if (typeof healNodeDisplay === 'function') healNodeDisplay(o);
        return o;
      }) : [];
      g.count = nodes.length;
      next[g.id] = {
        label: p.label || g.name,
        foot: p.foot || g.id + '.local',
        nodes,
      };
    }
    // drop orphan profiles not in GROUPS
    SUB_PROFILES = next;
    return true;
  }
  /**
   * Stale catalog from old edit-save: VMess tls.on + transport.ws missing path/Host
   * (iGG truth is plain WS). Cannot invent Host — strip false TLS so re-edit/hint is honest.
   * Full fix = 更新订阅.
   */
  function repairStoredOutbound(ob) {
    if (!ob || typeof ob !== 'object') return ob;
    let o;
    try { o = JSON.parse(JSON.stringify(ob)); } catch (_) { return ob; }
    const tr = o.transport && typeof o.transport === 'object' ? o.transport : null;
    const ty = String(o.type || '').toLowerCase();
    if (ty === 'vmess' && tr && (tr.type === 'ws' || tr.type === 'websocket')) {
      const host = (tr.headers && (tr.headers.Host || tr.headers.host)) || tr.host || '';
      const path = tr.path || '';
      if (!host && !path && o.tls && o.tls.enabled === true) {
        delete o.tls;
      }
    }
    if (typeof sanitizeSingBoxOutbound === 'function') {
      const s = sanitizeSingBoxOutbound(o);
      if (s) return s;
    }
    return o;
  }
  function stripDemoSeedNodes() {
    // old seeds: 备用 5 fake + Default CF官方优选*; product default is empty groups
    const BACKUP_DEMO = new Set(['备用-东京A', '备用-新加坡B', '备用-香港C', '备用-美西D', '备用-法兰克福E']);
    const bag = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES : null;
    if (!bag) return false;
    let changed = false;
    if (bag.backup && Array.isArray(bag.backup.nodes)) {
      const n0 = bag.backup.nodes.length;
      bag.backup.nodes = bag.backup.nodes.filter(n => n && !BACKUP_DEMO.has(n.name));
      if (bag.backup.nodes.length !== n0) {
        const g = (GROUPS || []).find(x => x.id === 'backup');
        if (g) g.count = bag.backup.nodes.length;
        changed = true;
      }
    }
    // only default group + CF官方优选* prefix (do not scan other groups)
    if (bag.default && Array.isArray(bag.default.nodes)) {
      const n0 = bag.default.nodes.length;
      bag.default.nodes = bag.default.nodes.filter(n => {
        if (!n || !n.name) return true;
        if (!String(n.name).startsWith('CF官方优选')) return true;
        // keep if user attached real connect payload
        if (n.link || n.outbound) return true;
        return false;
      });
      if (bag.default.nodes.length !== n0) {
        const g = (GROUPS || []).find(x => x.id === 'default');
        if (g) g.count = bag.default.nodes.length;
        changed = true;
      }
    }
    return changed;
  }
  function hideUnwiredSettings() {
    // routing / connection / decorative rows not wired to store or generate
    ['panel-routing', 'panel-vpn'].forEach(id => {
      const panel = document.getElementById(id);
      if (panel) {
        panel.hidden = true;
        panel.setAttribute('aria-hidden', 'true');
        panel.classList.remove('active');
      }
    });
    document.querySelectorAll('.side-item[data-settings="routing"], .side-item[data-settings="vpn"]').forEach(el => {
      el.hidden = true;
      el.setAttribute('aria-hidden', 'true');
      el.setAttribute('tabindex', '-1');
      el.style.display = 'none';
    });
    // Tun fine settings + launch/core/about/sub-update + menu NYI
    document.querySelectorAll('.set-unwired-tun, .set-unwired').forEach(el => {
      el.hidden = true;
      el.setAttribute('aria-hidden', 'true');
    });
  }
  function applyHydratedCatalog(data, src) {
    if (!data || !applyCatalog(data)) return false;
    const stripped = stripDemoSeedNodes();
    // Never persist empty shell over store just because hydrate painted defaults.
    // Persist when: demo stripped, or LS→store migrate, or catalog has nodes/URLs.
    const n = GROUPS.reduce((s, g) => s + (g.count | 0), 0);
    const hasUrl = GROUPS.some(g => g.url && String(g.url).trim());
    const shouldPut = stripped || src === 'local→store' || n > 0 || hasUrl;
    if (shouldPut) {
      try { if (typeof saveCatalog === 'function') saveCatalog(true); } catch (_) {}
    }
    if (typeof syncSubSegFromGroups === 'function') syncSubSegFromGroups();
    const want = data.active && GROUPS.some(g => g.id === data.active)
      ? data.active
      : (GROUPS[0] && GROUPS[0].id);
    if (want && typeof setActiveGroup === 'function') {
      setActiveGroup(want, { render: true, logIt: false });
    } else if (typeof renderNodes === 'function' && want) {
      renderNodes(want);
    }
    if (typeof syncSubSettingsFromActiveGroup === 'function') syncSubSettingsFromActiveGroup();
    try { log('SYS', 'info', t('log.subRestored', { groups: GROUPS.length, n, src: src ? ' · ' + src : '', stripped: stripped ? t('log.demoStripped') : '' })); } catch (_) {}
    return true;
  }

  async function hydrateCatalogOnBoot() {
    // store first; migrate LS → store once if store empty
    let fromStore = null;
    try {
      if (typeof nexusInvoke === 'function') {
        const r = await nexusInvoke('catalog_get');
        const blob = r && r.ok ? r.data : r;
        if (blob && typeof blob === 'object' && !Array.isArray(blob) && blob.v === 1) fromStore = blob;
      }
    } catch (_) {}
    const storeUseful = fromStore && Array.isArray(fromStore.groups) && fromStore.groups.length && (
      (fromStore.groups.some(g => g && g.url && String(g.url).trim())) ||
      (fromStore.profiles && Object.values(fromStore.profiles).some(p => p && Array.isArray(p.nodes) && p.nodes.length))
    );
    if (storeUseful && applyHydratedCatalog(fromStore, 'store')) return true;
    // empty/default store: try LS before painting empty groups

    const ls = loadCatalogFromLocal();
    if (ls && applyHydratedCatalog(ls, 'local→store')) {
      try {
        if (typeof nexusInvoke === 'function') {
          await nexusInvoke('catalog_put', { blob: serializeCatalog() });
          try { localStorage.removeItem(CATALOG_KEY); } catch (_) {}
        }
      } catch (_) {}
      return true;
    }
    if (typeof renderNodes === 'function') renderNodes(GROUPS[0]?.id || 'default');
    return false;
  }

  /** mac kill-switch: missing NexusFwD blocks connect — install once then retry. */
  function firewallHelperMissingErr(err) {
    return /firewall helper not running|NexusFwD|install via/i.test(String(err || ''));
  }
  async function connectSelectedWithHelper(connectArgs) {
    let r = await nexusInvoke('connect_selected', connectArgs);
    if (r && !r.offline && !r.ok && firewallHelperMissingErr(r.error)) {
      log('SYS', 'warn', t('fw.needHelper'));
      try {
        const ir = await nexusInvoke('firewall_helper_install');
        if (ir && ir.ok === false) throw new Error(ir.error || 'install failed');
        log('SYS', 'info', t('fw.installOk'));
        r = await nexusInvoke('connect_selected', connectArgs);
      } catch (e) {
        return {
          ok: false,
          error: t('fw.installFirst') + ': ' + String(e && e.message || e),
        };
      }
    }
    return r;
  }

  function paintFirewallStatus(st) {
    const tone = (el, kind) => {
      if (!el) return;
      el.classList.remove('ok', 'warn', 'err', 'neutral', 'muted');
      if (kind) el.classList.add(kind);
    };
    const setText = (id, v, kind) => {
      const el = document.getElementById(id);
      if (!el) return;
      const empty = v == null || v === '';
      el.textContent = empty ? '—' : String(v);
      tone(el, empty ? (el.classList.contains('fw-pill') ? 'neutral' : 'muted') : kind);
    };
    if (!st || typeof st !== 'object') {
      setText('fwState', '—', 'neutral');
      setText('fwSupport', '—', 'muted');
      setText('fwPolicy', '—', 'muted');
      setText('fwPeer', '—', 'muted');
      setText('fwTun', '—', 'muted');
      setText('fwDesired', '—', 'muted');
      setText('fwApplied', '—', 'muted');
      setText('fwErr', '—', 'muted');
      setText('fwHelper', '—', 'neutral');
      return;
    }
    const stateRaw = String(st.tunnel_state || '').toLowerCase();
    let stateTone = 'neutral';
    if (stateRaw === 'connected') stateTone = 'ok';
    else if (stateRaw === 'error' || stateRaw === 'blocked') stateTone = 'err';
    else if (stateRaw === 'connecting' || stateRaw === 'disconnecting') stateTone = 'warn';
    setText('fwState', st.tunnel_state, stateTone);

    const support = st.support === 'active'
      ? (t('fw.active') || st.support)
      : (st.support === 'unsupported' ? (t('fw.unsupported') || st.support) : st.support);
    setText('fwSupport', support, st.support === 'active' ? 'ok' : (st.support === 'unsupported' ? 'warn' : null));
    const mismatch = !!st.policy_mismatch;
    const pol = mismatch
      ? ((st.desired_policy || '—') + ' ≠ ' + (st.applied_policy || st.last_policy || '—'))
      : (st.last_policy || st.applied_policy || '—');
    setText('fwPolicy', pol, mismatch ? 'warn' : (st.last_policy ? null : 'muted'));
    setText('fwDesired', st.desired_policy, st.desired_policy ? (mismatch ? 'warn' : null) : 'muted');
    setText('fwApplied', st.applied_policy || st.last_policy, (st.applied_policy || st.last_policy) ? (mismatch ? 'warn' : null) : 'muted');
    setText('fwPeer', st.peer, st.peer ? null : 'muted');
    setText('fwTun', st.tun_if, st.tun_if ? null : 'muted');
    setText('fwErr', st.last_error, st.last_error ? 'err' : 'muted');

    const h = st.helper_running
      ? (t('fw.helperOn') || 'running')
      : (st.helper_installed
        ? (t('fw.helperOff') || 'installed, not running')
        : (t('fw.helperMissing') || 'not installed'));
    const helperTone = st.helper_running ? 'ok' : (st.helper_installed ? 'warn' : 'err');
    setText('fwHelper', st.helper_detail ? (h + ' · ' + st.helper_detail) : h, helperTone);

    const ins = document.getElementById('fwInstallBtn');
    const un = document.getElementById('fwUninstallBtn');
    if (ins) ins.disabled = !!st.helper_running;
    if (un) un.disabled = !(st.helper_installed || st.helper_running);
  }
  async function refreshFirewall() {
    try {
      const r = await nexusInvoke('firewall_status');
      // nexusInvoke wraps command result as { ok, data }
      if (!r || r.offline) {
        paintFirewallStatus({ last_error: t('log.backendDown') || 'backend offline' });
        return;
      }
      if (!r.ok) {
        paintFirewallStatus({ last_error: r.error || 'firewall_status failed' });
        return;
      }
      paintFirewallStatus(r.data || {});
    } catch (e) {
      paintFirewallStatus({ last_error: String(e && e.message || e) });
    }
  }

  function enterBlockView() {
    refreshFirewall();
  }
  (function bindFwHelperBtns() {
    const ins = document.getElementById('fwInstallBtn');
    const un = document.getElementById('fwUninstallBtn');
    if (ins && !ins._fwBound) {
      ins._fwBound = true;
      ins.addEventListener('click', async () => {
        try {
          const r = await nexusInvoke('firewall_helper_install');
          if (r && r.ok) {
            paintFirewallStatus(r.data || {});
            if (typeof log === 'function') log('SYS', 'info', t('fw.installOk') || 'firewall helper installed');
          } else {
            if (typeof log === 'function') log('SYS', 'error', String(r && (r.error || r.data) || 'install failed'));
            refreshFirewall();
          }
        } catch (e) {
          if (typeof log === 'function') log('SYS', 'error', String(e && e.message || e));
          refreshFirewall();
        }
      });
    }
    if (un && !un._fwBound) {
      un._fwBound = true;
      un.addEventListener('click', async () => {
        try {
          const r = await nexusInvoke('firewall_helper_uninstall');
          if (r && r.ok) {
            paintFirewallStatus(r.data || {});
            if (typeof log === 'function') log('SYS', 'info', t('fw.uninstallOk') || 'firewall helper removed');
          } else {
            if (typeof log === 'function') log('SYS', 'error', String(r && (r.error || r.data) || 'uninstall failed'));
            refreshFirewall();
          }
        } catch (e) {
          if (typeof log === 'function') log('SYS', 'error', String(e && e.message || e));
          refreshFirewall();
        }
      });
    }
  })();

  function openConnCtxMenu(x, y) {
    if (!connCtxMenu) return;
    closeMenus();
    closeCtxMenu();
    closeLogCtxMenu();
    connCtxMenu.hidden = false;
    connCtxMenu.classList.add('open');
    syncConnBlockMenuState();
    const w = connCtxMenu.offsetWidth || 120;
    const h = connCtxMenu.offsetHeight || 40;
    connCtxMenu.style.left = Math.max(8, Math.min(x, window.innerWidth - w - 8)) + 'px';
    connCtxMenu.style.top = Math.max(8, Math.min(y, window.innerHeight - h - 8)) + 'px';
  }
  function connRowText(row) {
    if (!row) return '';
    return [...row.cells].map(td => td.innerText.trim()).filter(Boolean).join('\t');
  }
  // progressive selection: click = single; ⌘/Ctrl = toggle; Shift = range from anchor
  let selectAnchorConnRow = null;
  function visibleConnRows() {
    return connTable ? [...connTable.querySelectorAll('tr')] : [];
  }
  function resolveConnSelectAnchor(rows) {
    if (selectAnchorConnRow && rows.includes(selectAnchorConnRow)) return selectAnchorConnRow;
    const id = selectAnchorConnRow?.dataset?.id;
    if (id) {
      const byId = rows.find(r => r.dataset.id === id);
      if (byId) {
        selectAnchorConnRow = byId;
        return byId;
      }
    }
    const sel = rows.find(r => r.classList.contains('selected'));
    if (sel) {
      selectAnchorConnRow = sel;
      return sel;
    }
    return null;
  }
  function selectConnRow(row, { multi, range } = {}) {
    if (!row || !connTable) return;
    const rows = visibleConnRows();
    if (range) {
      const anchor = resolveConnSelectAnchor(rows) || row;
      const a = rows.indexOf(anchor);
      const b = rows.indexOf(row);
      if (a >= 0 && b >= 0) {
        const lo = Math.min(a, b), hi = Math.max(a, b);
        if (!multi) rows.forEach(r => r.classList.remove('selected'));
        for (let i = lo; i <= hi; i++) rows[i].classList.add('selected');
        if (!selectAnchorConnRow || !rows.includes(selectAnchorConnRow)) selectAnchorConnRow = anchor;
      } else {
        if (!multi) rows.forEach(r => r.classList.remove('selected'));
        row.classList.add('selected');
        selectAnchorConnRow = row;
      }
    } else if (multi) {
      row.classList.toggle('selected');
      selectAnchorConnRow = row;
    } else {
      rows.forEach(r => r.classList.remove('selected'));
      row.classList.add('selected');
      selectAnchorConnRow = row;
    }
  }
  // Kill native text selection in the conn list (Shift otherwise selects glyphs, not rows)
  if (connTable) {
    connTable.addEventListener('selectstart', (e) => { e.preventDefault(); }, true);
    connTable.addEventListener('dragstart', (e) => { e.preventDefault(); }, true);
    connTable.addEventListener('mousedown', (e) => {
      if (!e.target.closest('tr')) return;
      e.preventDefault();
      clearDomTextSelection();
    }, true);
    connTable.addEventListener('click', (e) => {
      const row = e.target.closest('tr');
      if (!row || !connTable.contains(row)) return;
      if (e.shiftKey || e.metaKey || e.ctrlKey) clearDomTextSelection();
      selectConnRow(row, { multi: e.metaKey || e.ctrlKey, range: e.shiftKey });
    });
  }
  connPanel?.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (!dock.classList.contains('open')) setDockOpen(true);
    setDockPanel('conn');
    const row = e.target.closest('#connTable tr');
    connCtxRow = row || null;
    // match nodes: right-click already-selected keeps multi; else single-select that row
    if (row && !row.classList.contains('selected')) selectConnRow(row);
    if (!row) { closeConnCtxMenu(); return; }
    openConnCtxMenu(e.clientX, e.clientY);
  });
  function hostFromConnRow(row) {
    if (!row) return '';
    const raw = (row.dataset.dest || '').trim();
    if (!raw || raw === '—') return '';
    return raw;
  }
  function processPathFromConnRow(row) {
    if (!row) return '';
    const p = (row.dataset.proc || '').trim();
    if (!p || p === '—') return '';
    // basename-only is not enough for sing-box process_path match
    if (!p.includes('/') && !p.includes('\\')) return '';
    return p;
  }
  function setConnMenuItemState(btn, ready, label, titleOk, titleNo) {
    if (!btn) return;
    btn.disabled = !ready;
    btn.setAttribute('aria-disabled', ready ? 'false' : 'true');
    btn.title = ready ? titleOk : titleNo;
    btn.classList.toggle('is-disabled', !ready);
    const grow = btn.querySelector('.grow');
    if (grow && label != null) grow.textContent = label;
  }
  /** Selected rows for bulk block; fallback to right-clicked row. */
  function connBlockTargetRows() {
    const sel = visibleConnRows().filter(r => r.classList.contains('selected'));
    if (sel.length) return sel;
    return connCtxRow ? [connCtxRow] : [];
  }
  function syncConnBlockMenuState() {
    if (!connCtxMenu) return;
    const rows = connBlockTargetRows();
    let nHost = 0, nPath = 0, nHostPath = 0;
    for (const r of rows) {
      const h = !!hostFromConnRow(r);
      const p = !!processPathFromConnRow(r);
      if (h) nHost++;
      if (p) nPath++;
      if (h && p) nHostPath++;
    }
    const multi = (n) => (n > 1 ? t('js.itemsN', { n }) : '');
    // copy: multi-select when selected; else right-clicked row
    const copyRows = rows;
    let nCopyHost = 0;
    for (const r of copyRows) if (hostFromConnRow(r)) nCopyHost++;
    setConnMenuItemState(
      connCtxMenu.querySelector('[data-conn-ctx="copy-host"]'),
      nCopyHost > 0,
      t('conn.copyHost') + multi(nCopyHost),
      t('title.copyHost') + multi(nCopyHost),
      t('js.invalidDest')
    );
    setConnMenuItemState(
      connCtxMenu.querySelector('[data-conn-ctx="copy-row"]'),
      copyRows.length > 0,
      t('conn.copyRow') + multi(copyRows.length),
      t('title.copyRow') + multi(copyRows.length),
      t('log.noConnCopy')
    );
  }
  document.querySelectorAll('#connCtxMenu [data-conn-ctx]').forEach(btn => {
    btn.addEventListener('click', async () => {
      const act = btn.dataset.connCtx;
      if ((act === 'copy-host' ) && btn.disabled) {
        log('SYS', 'warn', t('log.invalidAddr'));
        closeConnCtxMenu();
        return;
      }
      closeConnCtxMenu();
      if (act === 'copy-host') {
        const rows = connBlockTargetRows();
        const hosts = [];
        const seen = new Set();
        for (const row of rows) {
          const host = hostFromConnRow(row);
          if (!host || seen.has(host)) continue;
          seen.add(host);
          hosts.push(host);
        }
        if (!hosts.length) { log('SYS', 'warn', t('log.invalidAddr')); return; }
        const text = hosts.join('\n');
        copyText(text).then(() => log('SYS', 'ok', hosts.length === 1
          ? t('log.copiedHost1', { host: hosts[0] })
          : t('log.copiedHostN', { n: hosts.length })))
          .catch(() => log('SYS', 'ok', t('log.copiedHost')));
      } else if (act === 'copy-row') {
        const rows = connBlockTargetRows();
        const lines = [];
        for (const row of rows) {
          const t = connRowText(row);
          if (t) lines.push(t);
        }
        if (!lines.length) { log('SYS', 'info', t('log.noConnCopy')); return; }
        const text = lines.join('\n');
        copyText(text).then(() => log('SYS', 'ok', lines.length === 1
          ? t('log.copiedRow1', { line: lines[0].replace(/\t/g, ' · ') })
          : t('log.copiedRowN', { n: lines.length })))
          .catch(() => log('SYS', 'ok', t('log.copiedRow')));
      }
    });
  });

  // ——— log filter bar ———
  let logLvl = 'all';
  function applyLogFilter() {
    const q = (document.getElementById('logFilterInput')?.value || '').trim().toLowerCase();
    logPanel.querySelectorAll('.log-line').forEach(line => {
      const lvl = line.dataset.lvl || 'info';
      const lvlOk = logLvl === 'all' || lvl === logLvl;
      const textOk = !q || line.innerText.toLowerCase().includes(q);
      line.hidden = !(lvlOk && textOk);
    });
  }
  document.getElementById('logFilterInput')?.addEventListener('input', applyLogFilter);
  document.querySelectorAll('#logFilter .lf-chip').forEach(chip => {
    chip.addEventListener('click', () => {
      document.querySelectorAll('#logFilter .lf-chip').forEach(c => c.classList.remove('active'));
      chip.classList.add('active');
      logLvl = chip.dataset.llvl || 'all';
      applyLogFilter();
    });
  });

  // ——— connection table: Core QueryConnections → rows ———
  /**
   * Finite-state conn cols: width = max(header, known labels) measured in paint font.
   * 出站 tags from generate (proxy/direct); 协议 = network (+ sniff) from Core metadata.
   */
  const CONN_OUT_LABELS = ['proxy', 'direct', '—'];
  // network × common sniff combos UI may paint as "tcp (http)"
  const CONN_PROTO_LABELS = [
    '—', 'tcp', 'udp',
    'tcp (http)', 'tcp (tls)', 'tcp (dns)', 'tcp (ssh)', 'tcp (rdp)',
    'udp (quic)', 'udp (dns)', 'udp (stun)', 'udp (dtls)', 'udp (bit-torrent)',
  ];
  let _connFiniteWReady = false;
  function measureTextPx(text, style) {
    const el = document.createElement('span');
    el.textContent = String(text ?? '');
    el.style.cssText = 'position:absolute;left:-9999px;top:0;visibility:hidden;white-space:nowrap;' + (style || '');
    document.body.appendChild(el);
    const w = Math.ceil(el.getBoundingClientRect().width);
    el.remove();
    return w;
  }
  function measureConnColPx(labels, headText, bodyStyle, headStyle) {
    let max = 0;
    for (const s of labels) max = Math.max(max, measureTextPx(s, bodyStyle));
    max = Math.max(max, measureTextPx(headText || '', headStyle));
    return max + 20; // pad 8+8 + sort chevron slack
  }
  function ensureConnFiniteColWidths() {
    if (_connFiniteWReady) return;
    const table = document.querySelector('.conn-table');
    if (!table) return;
    const sansBody = 'font:500 11px system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;';
    const sansHead = 'font:600 11px system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;';
    // proto cells use mono 11.5px (match .conn-table td)
    const monoBody = 'font:500 11.5px ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;';
    const monoHead = sansHead; // header stays sans 11/600
    const thOut = table.querySelector('th.rule');
    const thProto = table.querySelector('th.proto');
    const outHead = (thOut && thOut.textContent.trim()) || t('th.outbound');
    const protoHead = (thProto && thProto.textContent.trim()) || t('th.proto');
    table.style.setProperty('--conn-out-w', measureConnColPx(CONN_OUT_LABELS, outHead, sansBody, sansHead) + 'px');
    table.style.setProperty('--conn-proto-w', measureConnColPx(CONN_PROTO_LABELS, protoHead, monoBody, monoHead) + 'px');
    _connFiniteWReady = true;
  }
  let connSortKey = null, connSortDir = 1;
  let connPollTimer = null;
  // empty snapshots during Core Stop/Start can be brief; require 2 in a row while connected
  let connEmptyStreak = 0;
  function fmtBytes(n) {
    n = Number(n) || 0;
    if (n < 1024) return n + ' B';
    if (n < 1024 * 1024) return (n / 1024).toFixed(1) + ' KB';
    if (n < 1024 * 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + ' MB';
    return (n / (1024 * 1024 * 1024)).toFixed(2) + ' GB';
  }
  function fmtConnTime(ms) {
    const n = Number(ms) || 0;
    if (n <= 0) return '—';
    const d = new Date(n);
    if (Number.isNaN(d.getTime())) return '—';
    const p = (x) => String(x).padStart(2, '0');
    return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  }
  /** Stable row key: Core connection id; composite fallback if id missing. */
  function connRowKey(c) {
    const id = String(c?.id ?? '').trim();
    if (id) return id;
    const pid = Number(c?.process_id) || 0;
    return `_${Number(c?.created_at) || 0}|${c?.domain || c?.dest || ''}|${c?.process || ''}|${pid}|${c?.network || ''}|${c?.protocol || ''}|${c?.outbound || ''}`;
  }
  function paintConnRow(tr, c, key) {
    const tms = Number(c.created_at) || 0;
    const time = fmtConnTime(tms);
    const app = c.process || '—';
    const pid = Number(c.process_id) || 0;
    const pidText = pid > 0 ? String(pid) : '—';
    const procPath = (c.process_path && String(c.process_path).trim()) || '';
    const dest = c.domain || c.dest || '—';
    let proto = c.network || '';
    if (c.protocol) proto = proto ? `${proto} (${c.protocol})` : c.protocol;
    if (!proto) proto = '—';
    const out = c.outbound || '—';
    const up = Number(c.upload) || 0;
    const down = Number(c.download) || 0;
    const flow = `${fmtBytes(up)}↑ ${fmtBytes(down)}↓`;
    const flowBytes = up + down;
    tr.dataset.id = key;
    tr.dataset.time = String(tms);
    tr.dataset.app = app;
    tr.dataset.pid = pid > 0 ? String(pid) : '';
    tr.dataset.proc = procPath;
    tr.dataset.dest = dest;
    tr.dataset.proto = proto;
    tr.dataset.out = out;
    tr.dataset.flow = String(flowBytes);
    tr.title = procPath
      ? (pid > 0 ? `${procPath} (pid ${pid})` : procPath)
      : (pid > 0 ? `${app} (pid ${pid})` : app);
    if (tr.cells.length !== 7) {
      tr.innerHTML =
        `<td class="time">${escHtml(time)}</td>` +
        `<td class="app">${escHtml(app)}</td>` +
        `<td class="pid">${escHtml(pidText)}</td>` +
        `<td>${escHtml(dest)}</td>` +
        `<td class="proto">${escHtml(proto)}</td>` +
        `<td class="rule">${escHtml(out)}</td>` +
        `<td class="flow">${escHtml(flow)}</td>`;
      return;
    }
    const put = (i, text) => {
      if (tr.cells[i].textContent !== text) tr.cells[i].textContent = text;
    };
    put(0, time);
    put(1, app);
    put(2, pidText);
    put(3, dest);
    put(4, proto);
    put(5, out);
    put(6, flow);
  }
  /**
   * Diff/merge connection table by stable id:
   * add new, update existing in place, remove missing, never full wipe-rebuild.
   * Empty snapshot → clear (truthful). Query failure must not call this.
   */
  function renderConnections(list) {
    ensureConnFiniteColWidths();
    const tb = document.getElementById('connTable');
    if (!tb) return;
    const raw = Array.isArray(list) ? list : [];
    // last-wins dedupe so Core/active+closed never paints two rows for one id
    const byKey = new Map();
    for (const c of raw) byKey.set(connRowKey(c), c);
    if (!byKey.size) {
      if (tb.rows.length) tb.innerHTML = '';
      return;
    }
    const prev = new Map();
    for (const tr of tb.querySelectorAll('tr')) {
      const k = tr.dataset.id || '';
      if (k) prev.set(k, tr);
    }
    const seen = new Set();
    const frag = document.createDocumentFragment();
    for (const [key, c] of byKey) {
      seen.add(key);
      let tr = prev.get(key);
      if (!tr) tr = document.createElement('tr');
      paintConnRow(tr, c, key);
      frag.appendChild(tr);
    }
    for (const [key, tr] of prev) {
      if (!seen.has(key)) tr.remove();
    }
    tb.appendChild(frag);
    if (connSortKey) sortConnTable(connSortKey, connSortDir);
  }
  /**
   * Per-node cumulative traffic. Core QueryStats resets on every Stop/Start
   * (node switch, Tun re-Start), so UI keeps node totals and only adds Core deltas.
   * Explicit 重置流量 is the only zero path.
   */
  let _coreBaseUp = null, _coreBaseDown = null;
  function findNodeByConnectedName(name) {
    if (!name || typeof SUB_PROFILES === 'undefined') return null;
    for (const id of Object.keys(SUB_PROFILES || {})) {
      const nodes = SUB_PROFILES[id]?.nodes;
      if (!Array.isArray(nodes)) continue;
      const n = nodes.find(x => x && x.name === name);
      if (n) return n;
    }
    return null;
  }
  function paintNodeFlowCell(name, flowStr) {
    if (!name || !nodeTable || !flowStr) return;
    const row = [...nodeTable.querySelectorAll('tr')].find(r => r.dataset.name === name);
    if (!row) return;
    const el = row.querySelector('td.flow, .flow');
    if (!el) return;
    el.classList.remove('muted');
    el.classList.add('flow');
    const parts = flowStr.split('·').map(s => s.trim());
    const u = parts[0] || '';
    const d = parts[1] || '';
    el.innerHTML = `<span class="up">${u}</span>` + (d ? ` · <span class="down">${d}</span>` : '');
  }
  function sumAllNodeFlowBytes() {
    let total = 0;
    if (typeof SUB_PROFILES === 'undefined') return 0;
    for (const id of Object.keys(SUB_PROFILES || {})) {
      const nodes = SUB_PROFILES[id]?.nodes;
      if (!Array.isArray(nodes)) continue;
      for (const n of nodes) {
        total += Math.max(0, Number(n.flowUp) || 0) + Math.max(0, Number(n.flowDown) || 0);
      }
    }
    return total;
  }
  function refreshSbProxyFromNodes() {
    if (!sbProxy) return;
    if (!connected && sumAllNodeFlowBytes() === 0) {
      sbProxy.textContent = '—';
      return;
    }
    sbProxy.textContent = fmtBytes(sumAllNodeFlowBytes());
  }
  function applyNodeFlow(up, down) {
    up = Math.max(0, Number(up) || 0);
    down = Math.max(0, Number(down) || 0);
    // First sample after connect / explicit reset: baseline only (no double-count).
    if (_coreBaseUp == null || _coreBaseDown == null) {
      _coreBaseUp = up;
      _coreBaseDown = down;
      refreshSbProxyFromNodes();
      return;
    }
    // Core box recreated → counters restart lower; re-baseline, don't invent a drop.
    if (up < _coreBaseUp || down < _coreBaseDown) {
      _coreBaseUp = up;
      _coreBaseDown = down;
      refreshSbProxyFromNodes();
      return;
    }
    const dUp = up - _coreBaseUp;
    const dDown = down - _coreBaseDown;
    _coreBaseUp = up;
    _coreBaseDown = down;
    if (dUp === 0 && dDown === 0) {
      refreshSbProxyFromNodes();
      return;
    }
    const name = connectedName || '';
    const n = findNodeByConnectedName(name);
    if (n) {
      n.flowUp = Math.max(0, Number(n.flowUp) || 0) + dUp;
      n.flowDown = Math.max(0, Number(n.flowDown) || 0) + dDown;
      n.flow = `${fmtBytes(n.flowUp)}↑ · ${fmtBytes(n.flowDown)}↓`;
      paintNodeFlowCell(name, n.flow);
      if (typeof saveCatalog === 'function') saveCatalog(false);
    }
    refreshSbProxyFromNodes();
  }
  // 1A: consecutive poll/RPC failures while UI thinks connected → auto disconnect.
  let connPollFailStreak = 0;
  const CONN_POLL_FAIL_LIMIT = 3;
  async function refreshConnections() {
    if (!connected) {
      connEmptyStreak = 0;
      connPollFailStreak = 0;
      renderConnections([]);
      return;
    }
    let pollOk = false;
    try {
      const r = await nexusInvoke('query_connections');
      if (r && r.ok && r.data) {
        pollOk = true;
        const list = r.data.active || [];
        // table body only when conn dock visible
        const open = dock?.classList.contains('open');
        const onConn = document.querySelector('.dock-tab.active')?.dataset?.panel === 'conn';
        const empty = !Array.isArray(list) || list.length === 0;
        if (empty) {
          connEmptyStreak += 1;
          // hold last paint for one empty poll (~1.5s) during Core re-Start
          if (open && onConn && connEmptyStreak >= 2) renderConnections([]);
        } else {
          connEmptyStreak = 0;
          if (open && onConn) renderConnections(list);
        }
        const st = document.getElementById('stConn');
        // count unique keys (same rule as table), not raw array length
        let n = 0;
        if (!empty && Array.isArray(list)) {
          const ks = new Set();
          for (const c of list) ks.add(connRowKey(c));
          n = ks.size;
        } else if (empty && connEmptyStreak < 2) {
          n = document.getElementById('connTable')?.rows?.length || 0;
        } else {
          n = 0;
        }
        if (st) st.textContent = String(n);
      } else if (r && !r.ok && !r.offline) {
        // explicit RPC failure (core dead / not started)
        pollOk = false;
      } else if (r && r.offline) {
        pollOk = false;
      }
      // Node 流量: Core QueryStats deltas → per-node cumulative (survives switch).
      try {
        const st = await nexusInvoke('query_stats');
        if (st && st.ok && st.data) {
          applyNodeFlow(st.data.upload, st.data.download);
          pollOk = true;
        }
      } catch (_) { /* QueryStats optional if core mid-stop */ }
    } catch (_) { /* offline / core down */ }

    if (pollOk) {
      connPollFailStreak = 0;
      return;
    }
    connPollFailStreak += 1;
    if (connPollFailStreak < CONN_POLL_FAIL_LIMIT || !connected) return;
    // Core gone while UI still ON — full disconnect cleanup (sticky proxy risk).
    connPollFailStreak = 0;
    try { log('SYS', 'warn', t('log.coreLost') || 'Core lost — disconnecting'); } catch (_) {}
    try {
      if (typeof runSessionOp === 'function') {
        await runSessionOp('disconnect', async () => {
          try { await nexusInvoke('disconnect_selected'); } catch (_) {
            try { await nexusInvoke('core_stop'); } catch (_) {}
          }
          if (typeof setConnected === 'function') setConnected(false);
        });
      } else {
        try { await nexusInvoke('disconnect_selected'); } catch (_) {}
        if (typeof setConnected === 'function') setConnected(false);
      }
    } catch (_) {
      if (typeof setConnected === 'function') setConnected(false);
    }
  }
  function startConnPoll() {
    // restart timer only — do not wipe table (reconnect / setConnected re-entry)
    if (connPollTimer) { clearInterval(connPollTimer); connPollTimer = null; }
    // Re-baseline Core only — never wipe per-node totals on switch/reconnect.
    _coreBaseUp = null;
    _coreBaseDown = null;
    refreshConnections();
    connPollTimer = setInterval(() => {
      if (!connected) { stopConnPoll(); return; }
      refreshConnections(); // always poll for node flow; table render gated inside
    }, 1500);
  }
  function stopConnPoll() {
    if (connPollTimer) { clearInterval(connPollTimer); connPollTimer = null; }
    connEmptyStreak = 0;
    renderConnections([]); // real disconnect only
  }
  function parseConnFlow(row) {
    const t = row.dataset.flow;
    if (t != null && t !== '' && isFinite(Number(t))) return Number(t);
    // 7 cols: time|app|pid|dest|proto|out|flow — flow is cells[6]
    const txt = row.cells[6]?.textContent || '';
    const m = String(txt).match(/([\d.]+)\s*([KMG]?B)/i);
    if (!m) return -1;
    let n = parseFloat(m[1]);
    const u = m[2].toUpperCase();
    if (u.startsWith('K')) n *= 1024;
    else if (u.startsWith('M')) n *= 1024 * 1024;
    else if (u.startsWith('G')) n *= 1024 * 1024 * 1024;
    return n;
  }
  function connSortValue(row, key) {
    switch (key) {
      case 'time': {
        const n = Number(row.dataset.time);
        return Number.isFinite(n) ? n : -1;
      }
      case 'app': return (row.dataset.app || row.cells[1]?.textContent || '').toLowerCase();
      case 'pid': {
        const n = Number(row.dataset.pid);
        return Number.isFinite(n) && n > 0 ? n : -1;
      }
      case 'dest': return (row.dataset.dest || row.cells[3]?.textContent || '').toLowerCase();
      case 'proto': return (row.dataset.proto || row.cells[4]?.textContent || '').toLowerCase();
      case 'out': return (row.dataset.out || row.cells[5]?.textContent || '').toLowerCase();
      case 'flow': return parseConnFlow(row);
      default: return '';
    }
  }
  function sortConnTable(key, dir) {
    const tb = document.getElementById('connTable');
    if (!tb) return;
    const rows = [...tb.querySelectorAll('tr')];
    rows.sort((a, b) => {
      const va = connSortValue(a, key), vb = connSortValue(b, key);
      let cmp = (typeof va === 'number' && typeof vb === 'number') ? va - vb
        : String(va).localeCompare(String(vb), locale, { numeric: true });
      return cmp * dir;
    });
    const frag = document.createDocumentFragment();
    rows.forEach(r => frag.appendChild(r));
    tb.appendChild(frag);
  }
  document.querySelectorAll('.conn-table th.sortable').forEach(th => {
    const doSort = () => {
      const key = th.dataset.csort;
      if (connSortKey === key) connSortDir = -connSortDir;
      else { connSortKey = key; connSortDir = 1; }
      sortConnTable(connSortKey, connSortDir);
      document.querySelectorAll('.conn-table th.sortable').forEach(h => {
        if (h.dataset.csort === connSortKey) h.setAttribute('aria-sort', connSortDir === 1 ? 'ascending' : 'descending');
        else h.removeAttribute('aria-sort');
      });
    };
    th.addEventListener('click', doSort);
    th.addEventListener('keydown', e => {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); doSort(); }
    });
  });

  // ——— runtime stats ———
  // ponytail: uptime only while "connected"; traffic still comes from Core only
  let statsStarted = null;
  let lastExitIp = null;
  let lastCountry = null;

  /** "JP" → "🇯🇵 JP". Regional indicators are A-Z at a fixed offset, so no table. */
  function countryLabel(cc) {
    const c = String(cc || '').trim().toUpperCase();
    if (!/^[A-Z]{2}$/.test(c)) return null;
    const flag = String.fromCodePoint(...[...c].map(ch => 0x1F1E6 + ch.charCodeAt(0) - 65));
    return `${flag} ${c}`;
  }

  /** Exit IP through the tunnel. Any failure clears both cells — a stale or
   *  direct-path address here would be worse than an empty one. */
  async function refreshExitIp(live) {
    if (!live) { lastExitIp = null; lastCountry = null; return; }
    const r = await nexusInvoke('exit_ip_probe');
    if (!r || !r.ok) { lastExitIp = null; lastCountry = null; return; }
    lastExitIp = r.data?.ip || null;
    lastCountry = countryLabel(r.data?.country);
  }
  let lastFwErrLogged = '';
  async function fillRuntimeStats() {
    // eng 5A: status/uptime/proxy trust session_status.running only;
    // local `connected` is chrome — reconcile when poll disagrees.
    let coreLabel = t('stats.coreStopped');
    let coreRunning = false;
    try {
      if (typeof nexusInvoke === 'function') {
        const r = await nexusInvoke('session_status');
        const st = r && r.ok ? r.data : r;
        if (st && st.running) {
          coreRunning = true;
          coreLabel = (st.profile_id != null && st.profile_id >= 0)
            ? t('stats.coreRunningPid', { id: st.profile_id })
            : t('stats.coreRunning');
        } else if (st && st.process_alive) {
          coreLabel = t('stats.coreAliveIdle');
        } else {
          coreLabel = t('stats.coreStopped');
        }
        if (typeof setConnected === 'function') {
          if (coreRunning && !connected) {
            let pinName = '';
            try { pinName = localStorage.getItem('nexus.lastConnected') || ''; } catch (_) {}
            if (pinName) {
              connectedName = pinName;
              if (typeof selectedName !== 'undefined') selectedName = pinName;
            }
            setConnected(true, { pin: !!pinName });
          } else if (!coreRunning && connected) {
            setConnected(false);
            try { localStorage.removeItem('nexus.lastConnected'); } catch (_) {}
          }
        }
        if (st && st.firewall_error && st.firewall_error !== lastFwErrLogged) {
          lastFwErrLogged = st.firewall_error;
          log('SYS', 'warn', st.firewall_error);
          if (typeof refreshFirewall === 'function') refreshFirewall();
        }
      }
    } catch (e) {
      coreLabel = connected ? t('stats.coreNoReply') : t('stats.coreStopped');
    }
    document.getElementById('stCore').textContent = coreLabel;
    document.getElementById('stConn').textContent = String(document.querySelectorAll('#connTable tr').length);
    const px = document.getElementById('sbProxy')?.textContent;
    const dx = document.getElementById('sbDirect')?.textContent;
    const live = coreRunning;
    document.getElementById('stProxy').textContent = (live && px && px !== '—') ? px : '—';
    document.getElementById('stDirect').textContent = (live && dx && dx !== '—') ? dx : '—';
    if (live && statsStarted != null) {
      const sec = Math.floor((Date.now() - statsStarted) / 1000);
      const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), s = sec % 60;
      document.getElementById('stUptime').textContent = `${h}h ${String(m).padStart(2,'0')}m ${String(s).padStart(2,'0')}s`;
    } else {
      document.getElementById('stUptime').textContent = '—';
    }
    // Last known first so reopening paints instantly; the probe below corrects it.
    document.getElementById('stOutIp').textContent = lastExitIp || '—';
    document.getElementById('stCountry').textContent = lastCountry || '—';
    const g = (typeof GROUPS !== 'undefined') ? GROUPS.find(x => x.id === (typeof activeGroupId === 'function' ? activeGroupId() : 'default')) : null;
    document.getElementById('stNextSub').textContent = (g && g.url && g.autoUpdate) ? t('stats.nextSubPending') : '—';
    // Last: a round trip through the tunnel must not hold up every cell above it.
    await refreshExitIp(live);
    document.getElementById('stOutIp').textContent = lastExitIp || '—';
    document.getElementById('stCountry').textContent = lastCountry || '—';
  }
  function openStatsDialog() {
    fillRuntimeStats();
    openDialog('statsMask');
  }
  document.getElementById('statsClose')?.addEventListener('click', () => closeDialog('statsMask'));
  document.getElementById('statsRefresh')?.addEventListener('click', () => {
    fillRuntimeStats().then(() => log('SYS', 'info', t('log.runtimeRefreshed')));
  });
  document.getElementById('statsMask')?.addEventListener('click', e => {
    if (e.target.id === 'statsMask') closeDialog('statsMask');
  });

  // ——— export config ———
  function buildExportConfigLocal() {
    // UI-only sketch when Core generate_preview unavailable
    const nodes = [...nodeTable.querySelectorAll('tr')].slice(0, 8).map(r => ({
      type: (r.querySelector('.pill')?.textContent || 'vless').toLowerCase(),
      tag: r.dataset.name || 'node',
      server: (r.querySelector('.addr')?.textContent || '0.0.0.0:443').split(':')[0],
      server_port: parseInt((r.querySelector('.addr')?.textContent || ':443').split(':')[1] || '443', 10)
    }));
    const obj = {
      log: { level: 'info' },
      inbounds: [{ type: 'mixed', listen: '127.0.0.1', listen_port: mixedPort() }],
      outbounds: nodes.length
        ? nodes.concat([{ type: 'direct', tag: 'direct' }])
        : [{ type: 'direct', tag: 'direct' }],
      route: { final: nodes[0]?.tag || 'direct' },
      _note: 'local sketch — not Core generate_preview',
    };
    return JSON.stringify(obj, null, 2);
  }
  async function openExportDialog() {
    const pre = document.getElementById('exportPre');
    if (pre) pre.textContent = t('js.generating');
    openDialog('exportMask');
    try {
      const inv = window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke;
      if (typeof inv === 'function') {
        const payload = (typeof nodeConnectPayload === 'function')
          ? nodeConnectPayload(selectedName)
          : null;
        if (!payload) throw new Error(t('js.noNodePayload'));
        const cfg = await inv('generate_preview', payload);
        if (pre) pre.textContent = JSON.stringify(cfg, null, 2);
        log('SYS', 'ok', t('log.previewExported'));
        return;
      }
    } catch (e) {
      log('SYS', 'warn', t('log.previewFail', { error: e && e.message || e }));
    }
    if (pre) pre.textContent = buildExportConfigLocal();
    log('SYS', 'info', t('log.previewLocal'));
  }
  document.getElementById('exportClose')?.addEventListener('click', () => closeDialog('exportMask'));
  document.getElementById('exportCopy')?.addEventListener('click', () => {
    const t = document.getElementById('exportPre').textContent || '';
    copyText(t).then(() => log('SYS', 'ok', t('log.sbCopied'))).catch(() => log('SYS', 'info', t('log.copyFail')));
  });
  document.getElementById('exportMask')?.addEventListener('click', e => {
    if (e.target.id === 'exportMask') closeDialog('exportMask');
  });

  // stock group labels follow locale; user renames stay as stored
  const STOCK_BACKUP_NAMES = new Set(['备用', 'Backup', 'Резерв', '備用']);
  function groupDisplayName(g) {
    if (!g) return '';
    if (g.id === 'backup' && STOCK_BACKUP_NAMES.has(g.name)) return t('tb.backup');
    return g.name;
  }
  function activeGroupId() {
    return document.querySelector('#subSeg button.active')?.dataset.sub || GROUPS[0]?.id || 'default';
  }
  function setActiveGroup(id, { render = true, logIt = false } = {}) {
    // current_group switch → node list + subscription settings for that group
    const seg = document.getElementById('subSeg');
    if (!seg) return;
    const btn = seg.querySelector(`button[data-sub="${id}"]`);
    if (!btn) return;
    seg.querySelectorAll('button').forEach(b => {
      b.classList.remove('active');
      b.setAttribute('aria-selected', 'false');
    });
    btn.classList.add('active');
    btn.setAttribute('aria-selected', 'true');
    if (render && typeof renderNodes === 'function') renderNodes(id);
    if (logIt) {
      const g = GROUPS.find(x => x.id === id);
      log('SYS', 'info', t('log.subSwitched', { name: g?.name || id }));
    }
    if (document.getElementById('groupsMask')?.classList.contains('open')) renderGroupList();
    // settings 订阅 panel must follow top-bar group (URL + auto-update)
    if (typeof syncSubSettingsFromActiveGroup === 'function') syncSubSettingsFromActiveGroup();
    else if (typeof syncSubUrlField === 'function') syncSubUrlField();
    if (typeof saveCatalog === 'function') saveCatalog(); // persist active group id
  }
  function renderGroupList() {
    const list = document.getElementById('groupList');
    if (!list) return;
    const activeKey = activeGroupId();
    // Group name and id come from imported subscriptions — escape before interpolating.
    list.innerHTML = GROUPS.map(g => `
      <div class="group-row${g.id === activeKey ? ' active' : ''}" data-gid="${escHtml(g.id)}" role="listitem" tabindex="0">
        <span class="g-name" title="${escHtml(groupDisplayName(g))}">${escHtml(groupDisplayName(g))}</span>
        <span class="g-meta">${escHtml(g.count)} ${t('js.nodes')}</span>
        <span class="g-acts">
          <button type="button" class="btn-row" data-gact="edit" data-gid="${escHtml(g.id)}">${t('ctx.edit')}</button>
          <button type="button" class="btn-row" data-gact="rename" data-gid="${escHtml(g.id)}">${t('btn.rename')}</button>
          <button type="button" class="btn-row danger" data-gact="del" data-gid="${escHtml(g.id)}">${t('ctx.delete')}</button>
        </span>
      </div>`).join('');

    list.querySelectorAll('.group-row').forEach(row => {
      row.addEventListener('click', (e) => {
        if (e.target.closest('[data-gact]')) return;
        setActiveGroup(row.dataset.gid, { logIt: true });
      });
      row.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          setActiveGroup(row.dataset.gid, { logIt: true });
        }
      });
    });
    list.querySelectorAll('[data-gact]').forEach(btn => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        const id = btn.dataset.gid;
        const act = btn.dataset.gact;
        const g = GROUPS.find(x => x.id === id);
        if (!g) return;
        if (act === 'edit') openGroupEdit(id, 'edit');
        else if (act === 'rename') openGroupEdit(id, 'rename');
        else if (act === 'del') deleteGroup(id);
      });
    });
  }
  function syncSubSegFromGroups() {
    const seg = document.getElementById('subSeg');
    if (!seg) return;
    const cur = seg.querySelector('button.active')?.dataset.sub;
    const keep = GROUPS.some(g => g.id === cur) ? cur : GROUPS[0]?.id;
    seg.innerHTML = GROUPS.map((g, i) => {
      const on = g.id === keep || (!keep && i === 0);
      const tip = (g.id === 'backup' && STOCK_BACKUP_NAMES.has(g.name))
        ? t('title.subBackup')
        : t('title.subDrag');
      return `<button type="button" data-sub="${escHtml(g.id)}" role="tab" aria-selected="${on ? 'true' : 'false'}" class="${on ? 'active' : ''}" title="${tip}">${escHtml(groupDisplayName(g))}</button>`;
    }).join('');
    bindSubSegInteractions(seg);
    if (keep && typeof renderNodes === 'function') renderNodes(keep);
  }
  /** Reorder GROUPS so fromId sits immediately before toId. */
  function reorderGroupBefore(fromId, toId) {
    if (!fromId || !toId || fromId === toId || !Array.isArray(GROUPS)) return false;
    const i = GROUPS.findIndex(g => g.id === fromId);
    if (i < 0) return false;
    const [item] = GROUPS.splice(i, 1);
    const j = GROUPS.findIndex(g => g.id === toId);
    if (j < 0) {
      GROUPS.splice(i, 0, item); // restore
      return false;
    }
    GROUPS.splice(j, 0, item);
    return true;
  }
  /** Top-bar group tabs: click = switch; document pointer-drag = reorder (HTML5 DnD flaky in WKWebView). */
  function bindSubSegInteractions(seg) {
    if (!seg) return;
    let dragId = null;
    let startX = 0;
    let dragging = false;
    let suppressClick = false;
    let overId = null;
    let activePtr = null;

    const clearDragUi = () => {
      seg.classList.remove('seg-dragging');
      seg.querySelectorAll('button.dragging, button.drag-over').forEach(el => {
        el.classList.remove('dragging', 'drag-over');
        el.style.pointerEvents = '';
      });
    };

    const hitSubId = (x, y) => {
      const src = seg.querySelector('button.dragging');
      if (src) src.style.pointerEvents = 'none';
      let id = null;
      try {
        const hit = document.elementFromPoint(x, y);
        id = hit?.closest?.('#subSeg button[data-sub]')?.dataset?.sub || null;
      } catch (_) {}
      if (src) src.style.pointerEvents = '';
      return id;
    };

    const paintOver = (id) => {
      overId = (id && id !== dragId) ? id : null;
      seg.querySelectorAll('button.drag-over').forEach(el => {
        if (el.dataset.sub !== overId) el.classList.remove('drag-over');
      });
      if (overId) {
        const el = seg.querySelector('button[data-sub="' + CSS.escape(overId) + '"]');
        if (el) el.classList.add('drag-over');
      }
    };

    const onMove = (e) => {
      if (dragId == null || e.pointerId !== activePtr) return;
      if (!dragging) {
        if (Math.abs(e.clientX - startX) < 8) return;
        dragging = true;
        seg.classList.add('seg-dragging');
        const src = seg.querySelector('button[data-sub="' + CSS.escape(dragId) + '"]');
        if (src) src.classList.add('dragging');
      }
      e.preventDefault();
      paintOver(hitSubId(e.clientX, e.clientY));
    };

    const onUp = (e) => {
      if (dragId == null || (activePtr != null && e.pointerId !== activePtr)) return;
      document.removeEventListener('pointermove', onMove, true);
      document.removeEventListener('pointerup', onUp, true);
      document.removeEventListener('pointercancel', onUp, true);
      // final hit if pointer still over a tab
      if (dragging && e && e.clientX != null) paintOver(hitSubId(e.clientX, e.clientY));
      const from = dragId;
      const to = overId;
      const should = dragging && to && to !== from;
      // clear BEFORE commit — avoids lostpointercapture / rebuild reentry
      dragId = null;
      dragging = false;
      activePtr = null;
      overId = null;
      clearDragUi();
      if (!should) return;
      if (!reorderGroupBefore(from, to)) return;
      suppressClick = true;
      syncSubSegFromGroups();
      if (document.getElementById('groupsMask')?.classList.contains('open')) renderGroupList();
      if (typeof saveCatalog === 'function') saveCatalog(true);
      if (typeof log === 'function') {
        try { log('SYS', 'info', t('log.groupReordered')); } catch (_) {}
      }
    };

    seg.querySelectorAll('button[data-sub]').forEach(btn => {
      btn.removeAttribute('draggable');
      btn.setAttribute('data-tauri-drag-region', 'false');
      btn.addEventListener('click', (e) => {
        if (suppressClick) {
          e.preventDefault();
          e.stopPropagation();
          suppressClick = false;
          return;
        }
        setActiveGroup(btn.dataset.sub, { logIt: true });
      });
      btn.addEventListener('pointerdown', (e) => {
        if (e.button != null && e.button !== 0) return;
        // abandon any prior drag
        document.removeEventListener('pointermove', onMove, true);
        document.removeEventListener('pointerup', onUp, true);
        document.removeEventListener('pointercancel', onUp, true);
        dragId = btn.dataset.sub;
        startX = e.clientX;
        dragging = false;
        overId = null;
        activePtr = e.pointerId;
        document.addEventListener('pointermove', onMove, true);
        document.addEventListener('pointerup', onUp, true);
        document.addEventListener('pointercancel', onUp, true);
      });
    });
  }
  function groupLiveNodeName(gid) {
    // Tunnel/connect target still in this group → cannot delete yet.
    const prof = (typeof SUB_PROFILES !== 'undefined') ? SUB_PROFILES[gid] : null;
    const nodes = prof && Array.isArray(prof.nodes) ? prof.nodes : [];
    if (!nodes.length) return '';
    const names = new Set(nodes.map(n => n && n.name).filter(Boolean));
    if (connected && connectedName && names.has(connectedName)) return connectedName;
    // Connecting (or busy with this selection) to a node that lives here.
    if (powerBusy && selectedName && names.has(selectedName)) return selectedName;
    return '';
  }
  async function deleteGroup(id) {
    const g = GROUPS.find(x => x.id === id);
    if (!g) return;
    if (GROUPS.length <= 1) { log('SYS', 'warn', t('log.keepOneGroup')); return; }
    const liveNode = groupLiveNodeName(id);
    if (liveNode) {
      const blockMsg = t('confirm.deleteGroupLive', { name: groupDisplayName(g) || g.name, node: liveNode });
      await askConfirm(blockMsg, {
        title: t('confirm.deleteGroupTitle'),
        okText: t('btn.ok'),
        danger: false,
      });
      try { log('SYS', 'warn', t('log.groupLiveInUse', { name: groupDisplayName(g) || g.name, node: liveNode })); } catch (_) {}
      return;
    }
    const msg = t('confirm.deleteGroup', { name: g.name });
    const ok = await askConfirm(msg, {
      title: t('confirm.deleteGroupTitle'),
      okText: t('ctx.delete'),
      danger: true,
    });
    if (!ok) return;
    // re-check: list may have changed while dialog open
    if (!GROUPS.some(x => x.id === id)) return;
    if (GROUPS.length <= 1) { log('SYS', 'warn', t('log.keepOneGroup')); return; }
    const wasActive = activeGroupId() === id;
    GROUPS = GROUPS.filter(x => x.id !== id);
    if (typeof SUB_PROFILES !== 'undefined') delete SUB_PROFILES[id];
    syncSubSegFromGroups();
    if (wasActive && GROUPS[0]) setActiveGroup(GROUPS[0].id, { logIt: false });
    renderGroupList();
    if (typeof saveCatalog === 'function') saveCatalog(true);
    log('SYS', 'warn', t('log.groupDeleted', { name: g.name }));
  }
  function openGroupsDialog() {
    try {
      renderGroupList();
      setGroupUpdateStatus('', '');
    } catch (err) {
      try { log('SYS', 'warn', 'groups: ' + (err && err.message || err)); } catch (_) {}
    }
    openDialog('groupsMask');
  }
  document.getElementById('groupsBtn')?.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    openGroupsDialog();
  });
  document.getElementById('groupsClose')?.addEventListener('click', () => closeDialog('groupsMask'));
  document.getElementById('groupsMask')?.addEventListener('click', (e) => {
    if (e.target.id === 'groupsMask') closeDialog('groupsMask');
  });
  document.getElementById('groupAdd')?.addEventListener('click', () => openGroupEdit(null, 'create'));
  document.getElementById('groupUpdateAll')?.addEventListener('click', () => {
    // DialogManageGroups::on_update_all_clicked → UI_update_all_groups()
    updateSubscription(null, { all: true });
  });

  // ——— group edit / create / rename ———
  function openGroupEdit(id, mode = 'edit') {
    groupEditMode = mode;
    groupEditId = id;
    const title = document.getElementById('groupEditTitle');
    const sub = document.getElementById('groupEditSub');
    const nameEl = document.getElementById('geName');
    const urlEl = document.getElementById('geUrl');
    const auto = document.getElementById('geAuto');
    const urlLabel = urlEl?.closest('label');
    const autoLabel = auto?.closest('label');

    if (mode === 'create') {
      if (title) title.textContent = t('js.newGroup');
      if (sub) sub.textContent = t('js.newGroupSub');
      if (nameEl) nameEl.value = '';
      if (urlEl) urlEl.value = '';
      if (auto) { auto.classList.add('on'); auto.setAttribute('aria-pressed', 'true'); }
      if (urlLabel) urlLabel.style.display = '';
      if (autoLabel) autoLabel.style.display = '';
    } else {
      const g = GROUPS.find(x => x.id === id);
      if (!g) return;
      if (mode === 'rename') {
        if (title) title.textContent = t('js.renameGroup');
        if (sub) sub.textContent = t('js.renameGroupSub');
        if (nameEl) nameEl.value = g.name || '';
        if (urlLabel) urlLabel.style.display = 'none';
        if (autoLabel) autoLabel.style.display = 'none';
      } else {
        if (title) title.textContent = t('js.editGroup');
        if (sub) sub.textContent = t('js.editGroupSub');
        if (nameEl) nameEl.value = g.name || '';
        if (urlEl) urlEl.value = g.url || '';
        const on = g.autoUpdate !== false;
        if (auto) { auto.classList.toggle('on', on); auto.setAttribute('aria-pressed', String(on)); }
        if (urlLabel) urlLabel.style.display = '';
        if (autoLabel) autoLabel.style.display = '';
      }
    }
    openDialog('groupEditMask');
    setTimeout(() => {
      nameEl?.focus();
      nameEl?.select?.();
    }, 30);
  }
  document.getElementById('geCancel')?.addEventListener('click', () => closeDialog('groupEditMask'));
  document.getElementById('groupEditMask')?.addEventListener('click', e => {
    if (e.target.id === 'groupEditMask') closeDialog('groupEditMask');
  });
  document.getElementById('geName')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      document.getElementById('geSave')?.click();
    }
  });
  document.getElementById('geSave')?.addEventListener('click', () => {
    const name = (document.getElementById('geName')?.value || '').trim();
    if (!name) {
      log('SYS', 'warn', t('log.groupNameEmpty'));
      document.getElementById('geName')?.focus();
      return;
    }
    if (groupEditMode === 'create') {
      if (GROUPS.some(x => x.name === name)) {
        log('SYS', 'warn', t('log.groupNameDup'));
        return;
      }
      const id = 'g' + Date.now().toString(36);
      const url = (document.getElementById('geUrl')?.value || '').trim();
      const auto = document.getElementById('geAuto')?.classList.contains('on');
      GROUPS.push({ id, name, count: 0, url, autoUpdate: !!auto });
      if (typeof SUB_PROFILES !== 'undefined') {
        SUB_PROFILES[id] = { label: name, foot: id + '.local', nodes: [] };
      }
      syncSubSegFromGroups();
      setActiveGroup(id, { logIt: false });
      renderGroupList();
      closeDialog('groupEditMask');
      if (typeof saveCatalog === 'function') saveCatalog(true);
      log('SYS', 'ok', t('log.groupCreated', { name }));
      return;
    }
    const g = GROUPS.find(x => x.id === groupEditId);
    if (!g) { closeDialog('groupEditMask'); return; }
    if (GROUPS.some(x => x.id !== g.id && x.name === name)) {
      log('SYS', 'warn', t('log.groupNameDup'));
      return;
    }
    g.name = name;
    if (groupEditMode === 'edit') {
      g.url = (document.getElementById('geUrl')?.value || '').trim();
      g.autoUpdate = !!document.getElementById('geAuto')?.classList.contains('on');
    }
    if (typeof SUB_PROFILES !== 'undefined' && SUB_PROFILES[g.id]) {
      SUB_PROFILES[g.id].label = name;
    }
    syncSubSegFromGroups();
    renderGroupList();
    closeDialog('groupEditMask');
    if (typeof saveCatalog === 'function') saveCatalog(true);
    log('SYS', 'ok', groupEditMode === 'rename' ? t('log.groupRenamed', { name }) : t('log.groupSaved', { name, url: g.url ? ' · ' + g.url : '' }));
  });


  // ——— subscription URL field (settings → 订阅) ———
  function fieldText(el) {
    if (!el) return '';
    return (el.innerText || el.textContent || '').replace(/\u200b/g, '').trim();
  }
  function setFieldText(el, text) {
    if (!el) return;
    const t = (text || '').trim();
    if (!t) {
      el.innerHTML = '<span class="ph">https://…/sub</span>';
      el.classList.add('placeholder');
    } else {
      el.textContent = t;
      el.classList.remove('placeholder');
    }
    // contenteditable often scrolls caret-to-end after write → URL looks "floated" mid-string
    try { el.scrollLeft = 0; } catch (_) {}
  }
  function syncSubUrlField() {
    // kept for callers; full settings sync below
    syncSubSettingsFromActiveGroup();
  }
  function syncSubSettingsFromActiveGroup() {
    // Top-bar group ↔ 设置→订阅：URL + 自动更新开关跟随当前分组
    if (typeof GROUPS === 'undefined') return;
    const id = (typeof activeGroupId === 'function')
      ? activeGroupId()
      : (document.querySelector('#subSeg button.active')?.dataset.sub);
    const g = GROUPS.find(x => x.id === id) || GROUPS[0];
    const el = document.getElementById('subUrlField');
    if (el) setFieldText(el, g?.url || '');
    // auto-update switch in panel-sub (first data-switch under 自动更新订阅 row)
    const autoBtn = document.querySelector('#panel-sub .set-row .switch-only[data-switch]');
    if (autoBtn && g) {
      const on = !!g.autoUpdate;
      autoBtn.classList.toggle('on', on);
      autoBtn.setAttribute('aria-pressed', String(on));
    }
    // hint shows which group is bound
    const hint = document.querySelector('#panel-sub .set-row .set-hint');
    if (hint && g) {
      hint.textContent = t('hint.subUrlNamed', { name: g.name });
    }
  }
  function applySubUrlFromField() {
    const el = document.getElementById('subUrlField');
    if (!el || typeof GROUPS === 'undefined') return;
    let url = fieldText(el);
    // strip placeholder if still showing
    if (el.classList.contains('placeholder') || url === 'https://…/sub') url = '';
    url = url.trim();
    if (url && !/^https?:\/\//i.test(url) && !/^([a-z][a-z0-9+\-.]*):\/\//i.test(url)) {
      log('SYS', 'warn', t('log.subUrlHttp'));
      el.focus();
      return;
    }
    const id = (typeof activeGroupId === 'function') ? activeGroupId() : (document.querySelector('#subSeg button.active')?.dataset.sub);
    const g = GROUPS.find(x => x.id === id) || GROUPS[0];
    if (!g) { log('SYS', 'warn', t('log.noGroups')); return; }
    g.url = url;
    setFieldText(el, url);
    if (typeof saveCatalog === 'function') saveCatalog(true);
    log('SYS', 'ok', url ? t('log.subUrlSaved', { name: g.name }) : t('log.subUrlCleared', { name: g.name }));
    // saving URL often followed by AsyncUpdate when user expects nodes
    if (url && typeof updateSubscription === 'function') {
      updateSubscription(g.id);
    } else if (!url && typeof renderNodes === 'function') {
      // keep table; only clear url
    }
  }
  document.getElementById('subUrlApply')?.addEventListener('click', applySubUrlFromField);
  document.getElementById('subUrlField')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      applySubUrlFromField();
    }
  });
  // 自动更新订阅 switch → current group.autoUpdate
  document.querySelector('#panel-sub .set-row .switch-only[data-switch]')?.addEventListener('click', () => {
    requestAnimationFrame(() => {
      const btn = document.querySelector('#panel-sub .set-row .switch-only[data-switch]');
      if (!btn || typeof GROUPS === 'undefined') return;
      const id = activeGroupId();
      const g = GROUPS.find(x => x.id === id);
      if (!g) return;
      g.autoUpdate = btn.classList.contains('on') || btn.getAttribute('aria-pressed') === 'true';
      if (typeof saveCatalog === 'function') saveCatalog();
      log('SYS', 'info', t('log.groupAutoUp', { name: g.name, state: g.autoUpdate ? t('log.on') : t('log.off') }));
    });
  });
  // clear placeholder on focus/input handled by existing field helpers; ensure sync when opening 订阅 panel
  const _showViewSub = typeof showView === 'function' ? null : null;

  // Real OS tray is owned by the Tauri shell — no in-page tray mock.
  function refreshTrayMenu() { /* shell tray refreshes from store/session */ }

  const dock = document.getElementById('dock');
  const dockToggle = document.getElementById('dockToggle');
  const dockResizer = document.getElementById('dockResizer');
  const win = document.querySelector('.window');
  const sideCollapse = document.getElementById('sideCollapse');
  const DOCK_H_KEY = 'nexus.dockOpenH';
  const DOCK_H_MIN = 80;
  const DOCK_H_DEFAULT = 132;
  function dockMaxH() {
    // leave room for status + node list
    const home = document.getElementById('view-home');
    const h = (home && home.clientHeight) || win.clientHeight || 480;
    return Math.max(DOCK_H_MIN + 40, Math.floor(h * 0.72));
  }
  function clampDockH(px) {
    return Math.min(dockMaxH(), Math.max(DOCK_H_MIN, Math.round(px)));
  }
  function applyDockHeight(px, { save } = {}) {
    const h = clampDockH(px);
    dock.style.setProperty('--dock-open-h', h + 'px');
    if (save) {
      try { localStorage.setItem(DOCK_H_KEY, String(h)); } catch (_) {}
    }
    return h;
  }
  function loadDockHeight() {
    let n = DOCK_H_DEFAULT;
    try {
      const raw = localStorage.getItem(DOCK_H_KEY);
      if (raw != null && raw !== '') n = parseInt(raw, 10) || DOCK_H_DEFAULT;
    } catch (_) {}
    applyDockHeight(n);
  }
  loadDockHeight();
  function setDockOpen(open) {
    dock.classList.toggle('open', open);
    const active = document.querySelector('.dock-tab.active')?.dataset.panel || 'log';
    const connPanel = document.getElementById('connPanel');
    const logFilter = document.getElementById('logFilter');
    if (open) {
      logPanel.hidden = active !== 'log';
      if (connPanel) connPanel.hidden = active !== 'conn';
      if (logFilter) logFilter.hidden = active !== 'log';
    } else {
      logPanel.hidden = true;
      if (connPanel) connPanel.hidden = true;
      if (logFilter) logFilter.hidden = true;
    }
    dockToggle.setAttribute('aria-expanded', String(open));
    dockToggle.title = open ? t('title.dockCollapse') : t('title.dockExpand');
  }
  function setDockPanel(panel) {
    document.querySelectorAll('.dock-tab').forEach(t => {
      const on = t.dataset.panel === panel;
      t.classList.toggle('active', on);
    });
    const open = dock.classList.contains('open');
    logPanel.hidden = !(open && panel === 'log');
    const connPanel = document.getElementById('connPanel');
    if (connPanel) connPanel.hidden = !(open && panel === 'conn');
    const logFilter = document.getElementById('logFilter');
    if (logFilter) logFilter.hidden = !(open && panel === 'log');
    if (open && panel === 'conn' && connected && typeof refreshConnections === 'function') {
      refreshConnections();
    }
  }
  dockToggle.addEventListener('click', () => setDockOpen(!dock.classList.contains('open')));
  // drag top edge to resize open height; persist as default
  (function bindDockResize() {
    let startY = 0, startH = 0, dragging = false;
    function onMove(e) {
      if (!dragging) return;
      // drag up → taller dock
      const next = startH + (startY - e.clientY);
      applyDockHeight(next);
    }
    function onUp() {
      if (!dragging) return;
      dragging = false;
      dock.classList.remove('resizing');
      document.removeEventListener('pointermove', onMove);
      document.removeEventListener('pointerup', onUp);
      const h = parseInt(getComputedStyle(dock).getPropertyValue('--dock-open-h'), 10) || DOCK_H_DEFAULT;
      applyDockHeight(h, { save: true });
    }
    dockResizer.addEventListener('pointerdown', (e) => {
      if (!dock.classList.contains('open')) return;
      e.preventDefault();
      dragging = true;
      startY = e.clientY;
      startH = dock.getBoundingClientRect().height;
      dock.classList.add('resizing');
      document.addEventListener('pointermove', onMove);
      document.addEventListener('pointerup', onUp);
    });
  })();

  const SIDE_W_KEY = 'nexus.sidebarW';
  const SIDE_W_MIN = 176;
  const SIDE_W_DEFAULT = 252;
  const SIDE_W_COLLAPSED = 72;
  function sideMaxW() {
    const w = win.clientWidth || 720;
    // leave main usable (~320px min)
    return Math.max(SIDE_W_MIN + 40, Math.min(360, Math.floor(w * 0.45)));
  }
  function clampSideW(px) {
    return Math.min(sideMaxW(), Math.max(SIDE_W_MIN, Math.round(px)));
  }
  function applySidebarWidth(px, { save } = {}) {
    const w = clampSideW(px);
    win.style.setProperty('--sidebar', w + 'px');
    if (save) {
      try { localStorage.setItem(SIDE_W_KEY, String(w)); } catch (_) {}
    }
    return w;
  }
  function loadSidebarWidth() {
    let n = SIDE_W_DEFAULT;
    try {
      const raw = localStorage.getItem(SIDE_W_KEY);
      if (raw != null && raw !== '') n = parseInt(raw, 10) || SIDE_W_DEFAULT;
    } catch (_) {}
    applySidebarWidth(n);
  }
  loadSidebarWidth();
  (function bindSideResize() {
    const sideResizer = document.getElementById('sideResizer');
    if (!sideResizer) return;
    let startX = 0, startW = 0, dragging = false;
    function onMove(e) {
      if (!dragging) return;
      applySidebarWidth(startW + (e.clientX - startX));
    }
    function onUp() {
      if (!dragging) return;
      dragging = false;
      win.classList.remove('sidebar-resizing');
      document.removeEventListener('pointermove', onMove);
      document.removeEventListener('pointerup', onUp);
      const cur = parseInt(getComputedStyle(win).getPropertyValue('--sidebar'), 10) || SIDE_W_DEFAULT;
      applySidebarWidth(cur, { save: true });
    }
    sideResizer.addEventListener('pointerdown', (e) => {
      if (win.classList.contains('sidebar-collapsed')) return;
      e.preventDefault();
      e.stopPropagation();
      dragging = true;
      startX = e.clientX;
      startW = document.getElementById('sidebar')?.getBoundingClientRect().width || SIDE_W_DEFAULT;
      win.classList.add('sidebar-resizing');
      document.addEventListener('pointermove', onMove);
      document.addEventListener('pointerup', onUp);
    });
  })();

  function setSidebarCollapsed(collapsed) {
    win.classList.toggle('sidebar-collapsed', collapsed);
    if (sideCollapse) {
      sideCollapse.setAttribute('aria-expanded', String(!collapsed));
      sideCollapse.title = collapsed ? t('nav.expandTitle') : t('nav.collapseTitle');
      sideCollapse.setAttribute('aria-label', sideCollapse.title);
    }
    // Inline --sidebar (from drag/load) beats class CSS. Force 72px on collapse so
    // the grid track actually shrinks; expand restores remembered width.
    if (collapsed) win.style.setProperty('--sidebar', SIDE_W_COLLAPSED + 'px');
    else loadSidebarWidth();
  }
  sideCollapse?.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    setSidebarCollapsed(!win.classList.contains('sidebar-collapsed'));
  });

  document.querySelectorAll('.dock-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      if (!dock.classList.contains('open')) {
        dock.classList.add('open');
        dockToggle.setAttribute('aria-expanded', 'true');
      }
      setDockPanel(tab.dataset.panel || 'log');
    });
  });

  function showView(name, settingsPanel, focus) {
    const targetId = 'view-' + (name || 'home');
    let target = document.getElementById(targetId);
    if (!target) {
      target = document.getElementById('view-home');
    }
    document.querySelectorAll('.main > .view, .view').forEach(v => {
      const on = target && v === target;
      v.classList.toggle('active', on);
      // a11y only — paint is driven solely by .active (see CSS)
      if (on) v.removeAttribute('hidden');
      else v.setAttribute('hidden', '');
    });
    if (target) {
      target.classList.add('active');
      target.removeAttribute('hidden');
    }
    if ((name || 'home') === 'block' && typeof enterBlockView === 'function') {
      enterBlockView();
    }
    if ((name || 'home') === 'settings') {
      const panel = settingsPanel || 'basic';
      const map = { general: 'basic', subscription: 'sub', advanced: 'sub', connection: 'vpn', core: 'basic', system: 'basic', about: 'basic' };
      let key = map[panel] || panel;
      if (key === 'routing' || key === 'vpn') key = 'basic';
      // scope to settings only — block view must not share .set-panel
      document.querySelectorAll('#view-settings .set-panel').forEach(p => {
        p.classList.toggle('active', p.id === `panel-${key}`);
      });
      const head = document.getElementById('settingsHead');
      if (head) head.textContent = t('panel.' + (['basic', 'routing', 'vpn', 'sub'].includes(key) ? key : 'basic'));
      if (key === 'sub' && typeof syncSubUrlField === 'function') syncSubUrlField();
    }
  }

  const routeProfileSelect = document.getElementById('routeProfileSelect');
  const bypassCnSwitch = document.getElementById('bypassCnSwitch');
  if (routeProfileSelect && bypassCnSwitch) {
    new MutationObserver(() => {
      if ((routeProfileSelect.dataset.value || '') === '大陆直连') {
        bypassCnSwitch.classList.add('on');
        bypassCnSwitch.setAttribute('aria-pressed', 'true');
      }
    }).observe(routeProfileSelect, { attributes: true, attributeFilter: ['data-value'] });
  }
  document.getElementById('adblockSwitch')?.addEventListener('click', () => {
    setTimeout(() => {
      const on = document.getElementById('adblockSwitch')?.classList.contains('on');
      log('SYS', 'info', on ? t('log.adblockOn') : t('log.adblockOff'));
    }, 0);
  });



  document.querySelectorAll('[data-switch]').forEach(btn => {
    btn.addEventListener('click', () => {
      const on = !btn.classList.contains('on');
      btn.classList.toggle('on', on);
      btn.setAttribute('aria-pressed', String(on));
      markSettingsDirty();
      // Live preview: hide/show menu-bar icon immediately (same as theme/icon style).
      if (btn.id === 'hideTraySwitch' && typeof nexusInvoke === 'function') {
        nexusInvoke('set_hide_tray', { hide: on }).then((r) => {
          if (r && r.ok === false) {
            log('SYS', 'warn', (r.error || r.msg || 'tray') + '');
          } else {
            log('SYS', 'info', on ? t('log.trayHidden') : t('log.trayShown'));
          }
        }).catch(() => {});
      }
    });
  });

  document.getElementById('tunResetAddr')?.addEventListener('click', () => {
    const rows = document.querySelectorAll('#panel-vpn .set-row');
    rows.forEach(r => {
      const lab = r.querySelector('.set-label')?.textContent || '';
      const f = r.querySelector('.field');
      if (!f) return;
      if (lab.includes('IPv4') || lab.includes('IPv4')) { f.classList.remove('placeholder'); f.textContent = '172.19.0.1/24'; }
      if (lab.includes('IPv6')) { f.classList.remove('placeholder'); f.textContent = 'fdfe:dcba:9876::1/96'; }
    });
    log('SYS', 'ok', t('log.tunReset'));
    markSettingsDirty();
  });
  document.getElementById('tunTroubleshoot')?.addEventListener('click', () => {
    log('SYS', 'warn', t('log.tunTrouble'));
  });

  let locale = 'zh-CN';
  let settingsDirty = false;
  let settingsSnapshot = '';
  let pendingLeave = null; // callback after confirm


  const OPT_I18N = {
    '系统默认': 'opt.font.system',
    'System': 'opt.theme.system',
    '浅色': 'opt.theme.light',
    '深色': 'opt.theme.dark',
    '稳定版': 'opt.channel.stable',
    '测试版': 'opt.channel.beta',
    '完整规则': 'opt.route.full',
    '大陆直连': 'opt.route.cn',
    '规则': 'opt.mode.rule',
    '全局': 'opt.mode.global',
    '直连': 'opt.mode.direct',
  };
  function optLabel(canonical) {
    const k = OPT_I18N[canonical];
    return k ? t(k) : canonical;
  }
  function refreshSelectLabels() {
    document.querySelectorAll('button.select').forEach(btn => {
      const val = btn.dataset.value || '';
      const el = btn.querySelector('.sel-val');
      if (el && val) el.textContent = optLabel(val);
    });
  }

  function t(key, vars) {
    const pack = I18N[locale] || I18N['zh-CN'];
    let s = pack[key] ?? I18N['zh-CN'][key] ?? key;
    if (vars) Object.keys(vars).forEach(k => { s = s.replaceAll('{' + k + '}', vars[k]); });
    return s;
  }

  /** t() for innerHTML sinks. The pack template is ours and keeps its <strong>;
   *  the vars are node/group names from subscriptions, so they get escaped.
   *  Use plain t() for textContent — escaping there would show literal &amp;. */
  function tHtml(key, vars) {
    if (!vars) return t(key);
    const safe = {};
    Object.keys(vars).forEach(k => { safe[k] = escHtml(vars[k]); });
    return t(key, safe);
  }
  function applyLocale(langLabel, { logIt } = {}) {
    const code = LANG_MAP[langLabel] || 'zh-CN';
    locale = code;
    document.documentElement.lang = code;
    document.querySelectorAll('[data-i18n]').forEach(el => {
      const key = el.getAttribute('data-i18n');
      if (!key) return;
      el.innerHTML = t(key);
    });
    document.querySelectorAll('[data-i18n-title]').forEach(el => {
      const key = el.getAttribute('data-i18n-title');
      if (!key) return;
      el.title = t(key);
      if (el.hasAttribute('aria-label')) {
        el.setAttribute('aria-label', t(key));
      }
    });
    document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
      const key = el.getAttribute('data-i18n-placeholder');
      if (!key) return;
      el.setAttribute('placeholder', t(key));
    });
    document.querySelectorAll('[data-i18n-html]').forEach(el => {
      const key = el.getAttribute('data-i18n-html');
      if (!key) return;
      el.innerHTML = t(key);
    });
    // dynamic status strings (connected hero ≠ list selection)
    if (typeof refreshHeroStatus === 'function') refreshHeroStatus();
    else {
      statusText.textContent = connected ? t('status.connected') : t('status.disconnected');
      statusSub.innerHTML = connected
        ? tHtml('status.subOn', { name: connectedName || selectedName, lat: connectedLat || selectedLat })
        : tHtml('status.subOff', { name: selectedName });
      sbStatus.textContent = connected ? t('sb.running') : t('sb.stopped');
    }
    const head = document.getElementById('settingsHead');
    const activePanel = document.querySelector('#view-settings .set-panel.active');
    if (head && activePanel) {
      const key = (activePanel.id || '').replace('panel-', '');
      head.textContent = t('panel.' + (['basic', 'routing', 'vpn', 'sub'].includes(key) ? key : 'basic'));
    }
    // language option display stays native names; optional localized in log
    if (typeof refreshSelectLabels === 'function') refreshSelectLabels();
    if (typeof syncSubSegFromGroups === 'function') syncSubSegFromGroups();
    if (typeof renderGroupList === 'function') renderGroupList();
    const sc = document.getElementById('sideCollapse');
    if (tableCard) tableCard.setAttribute('aria-label', t('aria.nodeList'));
    if (sc) {
      const collapsed = document.getElementById('window')?.classList.contains('sidebar-collapsed')
        || document.body.classList.contains('sidebar-collapsed')
        || document.querySelector('.window.sidebar-collapsed');
      const isCollapsed = !!(document.querySelector('.window.sidebar-collapsed'));
      sc.title = isCollapsed ? t('nav.expandTitle') : t('nav.collapseTitle');
      sc.setAttribute('aria-label', sc.title);
    }
    if (logIt) log('SYS', 'info', t('log.lang', { lang: langLabel }));
  }
  const FONT_MAP = {
    '系统默认': '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", "Helvetica Neue", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif',
    'SF Pro': '"SF Pro Text", "SF Pro Display", -apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif',
    'PingFang SC': '"PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", -apple-system, sans-serif',
  };
  function applyFont(fontLabel, { logIt } = {}) {
    const stack = FONT_MAP[fontLabel] || FONT_MAP['系统默认'];
    document.documentElement.style.setProperty('--font', stack);
    document.body.style.fontFamily = stack;
    if (logIt) log('SYS', 'info', t('log.font', { font: fontLabel }));
  }
  let themeMedia = null;
  function resolveTheme(label) {
    if (label === '深色') return 'dark';
    if (label === '浅色') return 'light';
    // System
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  function applyTheme(themeLabel, { logIt } = {}) {
    const mode = resolveTheme(themeLabel);
    document.documentElement.setAttribute('data-theme', mode);
    // follow OS only when System
    if (themeMedia) {
      themeMedia.removeEventListener('change', themeMedia._handler);
      themeMedia = null;
    }
    if (themeLabel === 'System') {
      themeMedia = window.matchMedia('(prefers-color-scheme: dark)');
      themeMedia._handler = () => {
        document.documentElement.setAttribute('data-theme', themeMedia.matches ? 'dark' : 'light');
        const iconSel = document.getElementById('iconSelect');
        if (iconSel && (iconSel.dataset.value || '') === 'Auto') applyIconStyle('Auto');
      };
      themeMedia.addEventListener('change', themeMedia._handler);
    }
    if (logIt) log('SYS', 'info', t('log.theme', { theme: themeLabel }));
    // re-resolve Auto icons when theme flips
    const iconSel = document.getElementById('iconSelect');
    if (iconSel && (iconSel.dataset.value || '') === 'Auto') applyIconStyle('Auto');
  }
  function resolveIconMode(label) {
    if (label === 'Colorful') return 'colorful';
    // Monochrome + Auto → mono (formal default)
    return 'mono';
  }
  function applyIconStyle(iconLabel, { logIt } = {}) {
    const mode = resolveIconMode(iconLabel);
    document.documentElement.setAttribute('data-icons', mode);
    document.documentElement.setAttribute('data-icons-pref', iconLabel);
    if (logIt) log('SYS', 'info', t('log.icons', { icons: iconLabel }));
  }

  function snapshotSettings() {
    const parts = [];
    document.querySelectorAll('#view-settings button.select').forEach(b => {
      parts.push('s:' + (b.id || b.dataset.options) + '=' + (b.dataset.value || ''));
    });
    document.querySelectorAll('#view-settings [data-switch]').forEach((b, i) => {
      parts.push('w:' + (b.id || i) + '=' + b.classList.contains('on'));
    });
    document.querySelectorAll('#view-settings .field[contenteditable="true"]').forEach((f, i) => {
      parts.push('f:' + i + '=' + (f.classList.contains('placeholder') ? '' : f.textContent.trim()));
    });
    return parts.join('|');
  }
  function setDirty(on) {
    settingsDirty = !!on;
    document.getElementById('settingsDirtyBar')?.classList.toggle('show', !!on);
  }
  function markSettingsDirty() {
    if (document.getElementById('view-settings')?.classList.contains('active')) {
      setDirty(snapshotSettings() !== settingsSnapshot);
    }
  }
  function beginSettingsSession() {
    settingsSnapshot = snapshotSettings();
    setDirty(false);
  }
  // Side nav: capture on document so SVG/label clicks always reach handler; never dirty-gated.
  function onSideNavClick(e) {
    const t = e.target;
    if (!t || !t.closest) return;
    const btn = t.closest('#sidebar .side-item');
    if (!btn) return;
    if (btn.hidden || btn.getAttribute('aria-hidden') === 'true') return;
    e.preventDefault();
    e.stopPropagation();
    const nextView = btn.dataset.view || 'home';
    const settingsEl = document.getElementById('view-settings');
    const inSettings = !!(settingsEl && settingsEl.classList.contains('active'));

    document.querySelectorAll('#sidebar .side-item').forEach(b => {
      b.classList.remove('active');
      b.removeAttribute('aria-current');
    });
    btn.classList.add('active');
    btn.setAttribute('aria-current', 'page');

    showView(nextView, btn.dataset.settings || 'basic', btn.dataset.focus || null);

    try {
      if (typeof closeConfirm === 'function') closeConfirm();
      pendingLeave = null;
      if (nextView !== 'settings') setDirty(false);
      else if (!inSettings) beginSettingsSession();
      else if (btn.dataset.settings === 'sub' && !settingsDirty) beginSettingsSession();
      if (nextView === 'block' && typeof ensureBlocklistOnView === 'function') ensureBlocklistOnView();
    } catch (_) { /* view already switched */ }
  }
  function bindSideNav() {
    if (document.documentElement.dataset.sideNavBound === '1') return;
    document.documentElement.dataset.sideNavBound = '1';
    document.addEventListener('click', onSideNavClick, true);
  }
  bindSideNav();
  function forceLeaveHome() {
    showView('home');
    document.querySelectorAll('.sidebar .side-item').forEach(b => {
      b.classList.remove('active');
      b.removeAttribute('aria-current');
    });
    const home = document.querySelector('.side-item[data-view="home"]');
    if (home) {
      home.classList.add('active');
      home.setAttribute('aria-current', 'page');
    }
    setDirty(false);
  }
  const UI_PREF_KEYS = [
    ['langSelect', 'nexus.lang'],
    ['fontSelect', 'nexus.font'],
    ['themeSelect', 'nexus.theme'],
    ['iconSelect', 'nexus.icon'],
  ];
  function saveUiPrefs() {
    UI_PREF_KEYS.forEach(([id, key]) => {
      const v = document.getElementById(id)?.dataset.value;
      if (!v) return;
      try { localStorage.setItem(key, v); } catch (_) {}
    });
  }
  function loadUiPrefs() {
    UI_PREF_KEYS.forEach(([id, key]) => {
      const btn = document.getElementById(id);
      if (!btn) return;
      let raw = null;
      try { raw = localStorage.getItem(key); } catch (_) {}
      if (!raw) return;
      const allowed = (btn.dataset.options || '').split('|');
      if (!allowed.includes(raw)) return;
      btn.dataset.value = raw;
      const val = btn.querySelector('.sel-val');
      if (val) val.textContent = (typeof optLabel === 'function') ? optLabel(raw) : raw;
    });
  }
  function saveSettings() {
    settingsSnapshot = snapshotSettings();
    saveUiPrefs();
    setDirty(false);
    log('SYS', 'ok', t('log.saved'));
  }
  function discardSettings() {
    // restore snapshot values
    const map = {};
    settingsSnapshot.split('|').forEach(p => {
      const i = p.indexOf('=');
      if (i > 0) map[p.slice(0, i)] = p.slice(i + 1);
    });
    document.querySelectorAll('#view-settings button.select').forEach(b => {
      const k = 's:' + (b.id || b.dataset.options);
      if (k in map) {
        b.dataset.value = map[k];
        const val = b.querySelector('.sel-val');
        if (val) val.textContent = (typeof optLabel === 'function') ? optLabel(map[k]) : map[k];
        if (b.id === 'langSelect') applyLocale(map[k]);
        if (b.id === 'fontSelect') applyFont(map[k]);
        if (b.id === 'themeSelect') applyTheme(map[k]);
        if (b.id === 'iconSelect') applyIconStyle(map[k]);
      }
    });
    document.querySelectorAll('#view-settings [data-switch]').forEach((b, i) => {
      const k = 'w:' + (b.id || i);
      if (k in map) {
        const on = map[k] === 'true';
        b.classList.toggle('on', on);
        b.setAttribute('aria-pressed', String(on));
        if (b.id === 'hideTraySwitch' && typeof nexusInvoke === 'function') {
          nexusInvoke('set_hide_tray', { hide: on }).catch(() => {});
        }
      }
    });
    document.querySelectorAll('#view-settings .field[contenteditable="true"]').forEach((f, i) => {
      const k = 'f:' + i;
      if (k in map) {
        const v = map[k];
        if (!v) {
          const ph = f.dataset.placeholder || '';
          f.classList.add('placeholder');
          f.innerHTML = ph ? `<span class="ph">${ph}</span>` : '';
        } else {
          f.classList.remove('placeholder');
          f.textContent = v;
        }
      }
    });
    setDirty(false);
    log('SYS', 'info', t('log.discard'));
  }
  function openConfirm(then) {
    pendingLeave = then;
    const mask = document.getElementById('confirmMask');
    if (!mask) { then && then('discard'); return; }
    mask.removeAttribute('hidden');
    mask.hidden = false;
    mask.classList.add('open');
  }
  function closeConfirm() {
    const mask = document.getElementById('confirmMask');
    if (!mask) return;
    mask.classList.remove('open');
    mask.hidden = true;
    mask.setAttribute('hidden', '');
  }
  function requestLeaveSettings(then) {
    if (!settingsDirty) {
      then && then('clean');
      return;
    }
    openConfirm(then);
  }
  function closeSettings() {
    requestLeaveSettings((action) => {
      if (action === 'save') saveSettings();
      else if (action === 'discard') discardSettings();
      forceLeaveHome();
    });
  }
  document.getElementById('settingsCancel')?.addEventListener('click', closeSettings);
  document.getElementById('settingsOk')?.addEventListener('click', () => {
    saveSettings();
    forceLeaveHome();
  });
  document.getElementById('settingsDiscard')?.addEventListener('click', () => {
    discardSettings();
  });
  document.getElementById('confirmSave')?.addEventListener('click', () => {
    closeConfirm();
    const cb = pendingLeave; pendingLeave = null;
    cb && cb('save');
  });
  document.getElementById('confirmDiscard')?.addEventListener('click', () => {
    closeConfirm();
    const cb = pendingLeave; pendingLeave = null;
    cb && cb('discard');
  });
  document.getElementById('confirmCancel')?.addEventListener('click', () => {
    closeConfirm();
    pendingLeave = null;
  });
  document.getElementById('confirmMask')?.addEventListener('click', (e) => {
    if (e.target.id === 'confirmMask') {
      closeConfirm();
      pendingLeave = null;
    }
  });


  // Pure-DOM selects: fixed popup (escapes overflow), width fits longest option
  function closeSelectPops() {
    document.querySelectorAll('.select-pop.open').forEach(p => {
      p.classList.remove('open');
      p.style.top = '';
      p.style.left = '';
      p.style.minWidth = '';
    });
  }
  document.querySelectorAll('button.select').forEach(btn => {
    const wrap = document.createElement('div');
    wrap.className = 'select-wrap';
    btn.parentNode.insertBefore(wrap, btn);
    wrap.appendChild(btn);
    const pop = document.createElement('div');
    pop.className = 'select-pop';
    pop.setAttribute('role', 'listbox');
    // body-level so settings scroll/overflow cannot clip
    document.body.appendChild(pop);

    function longestOpt(opts) {
      return opts.reduce((a, b) => (b.length > a.length ? b : a), '') || '';
    }
    function measureMinWidth(opts) {
      const probe = document.createElement('span');
      probe.style.cssText = 'position:absolute;visibility:hidden;white-space:nowrap;font:500 13px ' + getComputedStyle(btn).fontFamily;
      probe.textContent = longestOpt(opts);
      document.body.appendChild(probe);
      const need = Math.ceil(probe.getBoundingClientRect().width) + 44;
      document.body.removeChild(probe);
      return need;
    }
    function renderOpts() {
      const opts = (btn.dataset.options || '').split('|').filter(Boolean);
      const cur = btn.dataset.value || opts[0] || '';
      pop.innerHTML = '';
      opts.forEach(o => {
        const item = document.createElement('button');
        item.type = 'button';
        item.className = 'select-opt' + (o === cur ? ' active' : '');
        item.setAttribute('role', 'option');
        item.setAttribute('aria-selected', String(o === cur));
        item.textContent = (typeof optLabel === 'function') ? optLabel(o) : o;
        item.addEventListener('click', (e) => {
          e.stopPropagation();
          btn.dataset.value = o;
          const val = btn.querySelector('.sel-val');
          if (val) val.textContent = (typeof optLabel === 'function') ? optLabel(o) : o;
          closeSelectPops();
          if (btn.id === 'langSelect') {
            applyLocale(o, { logIt: true });
          } else if (btn.id === 'fontSelect') {
            applyFont(o, { logIt: true });
          } else if (btn.id === 'themeSelect') {
            applyTheme(o, { logIt: true });
          } else if (btn.id === 'iconSelect') {
            applyIconStyle(o, { logIt: true });
          } else if (btn.id === 'editType') {
            if (typeof syncEditFieldsByType === 'function') syncEditFieldsByType(o);
          }
          markSettingsDirty();
        });
        pop.appendChild(item);
      });
      return opts;
    }
    function placePop(opts) {
      const r = btn.getBoundingClientRect();
      const minW = Math.max(Math.ceil(r.width), measureMinWidth(opts));
      pop.style.minWidth = minW + 'px';
      let top = r.bottom + 4;
      let left = r.left;
      // open upward if near bottom
      const estH = Math.min(220, opts.length * 30 + 12);
      if (top + estH > window.innerHeight - 8) {
        top = Math.max(8, r.top - estH - 4);
      }
      if (left + minW > window.innerWidth - 8) {
        left = Math.max(8, window.innerWidth - minW - 8);
      }
      pop.style.top = top + 'px';
      pop.style.left = left + 'px';
    }

    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const willOpen = !pop.classList.contains('open');
      closeSelectPops();
      closeMenus();
      if (willOpen) {
        const opts = renderOpts();
        if (!opts.length) return;
        placePop(opts);
        pop.classList.add('open');
      }
    });
  });
  document.addEventListener('click', () => closeSelectPops());
  document.querySelector('.settings-body')?.addEventListener('scroll', () => closeSelectPops(), { passive: true });
  window.addEventListener('resize', () => closeSelectPops());

  // contenteditable fields placeholder class
  document.querySelectorAll('.field[contenteditable="true"]').forEach(el => {
    const ph = el.dataset.placeholder || '';
    const setEmpty = () => {
      el.classList.add('placeholder');
      el.innerHTML = ph ? `<span class="ph">${ph}</span>` : '';
    };
    const clearPh = () => {
      if (el.classList.contains('placeholder')) {
        el.textContent = '';
        el.classList.remove('placeholder');
      }
    };
    el.addEventListener('focus', clearPh);
    el.addEventListener('blur', () => {
      if (!el.textContent.trim()) setEmpty();
      try { el.scrollLeft = 0; } catch (_) {}
    });
  });

  let SUB_PROFILES = {
    default: {
      label: 'Default',
      foot: t('js.noSubFoot'),
      nodes: []
    },
    backup: {
      label: '备用',
      foot: t('js.noSubFoot'),
      nodes: []
    }
  };
  function latClass(ms) {
    if (ms == null || !isFinite(ms)) return 'lat';
    if (ms < 175) return 'lat good';
    if (ms < 220) return 'lat mid';
    return 'lat bad';
  }
  function escHtml(s) {
    return String(s ?? '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }
  function renderNodes(subKey) {
    const prof = SUB_PROFILES[subKey] || SUB_PROFILES.default;
    const rows = prof.nodes.map((n, i) => {
      let latTxt = '—';
      let latCls = 'lat';
      if (n.lat != null && isFinite(n.lat)) {
        if (n.lat < 0) { latTxt = 'timeout'; latCls = 'lat bad'; }
        else { latTxt = n.lat + ' ms'; latCls = latClass(n.lat); }
      }
      let flowHtml = '—';
      let flowCls = 'flow muted';
      if (n.flow) {
        flowCls = 'flow';
        const parts = n.flow.split('·').map(s => s.trim());
        const up = parts[0] || '';
        const down = parts[1] || '';
        flowHtml = `<span class="up">${escHtml(up)}</span>` + (down ? ` · <span class="down">${escHtml(down)}</span>` : '');
      }
      const nm = n.name || '';
      const ad = n.addr || '';
      const ty = n.type || '';
      return `<tr data-name="${escHtml(nm)}" data-lat="${escHtml(latTxt)}"${i===0?' class="selected"':''}>
                  <td class="idx">${i+1}</td>
                  <td><span class="pill">${escHtml(ty)}</span></td>
                  <td class="addr" title="${escHtml(ad)}">${escHtml(ad)}</td>
                  <td class="name" title="${escHtml(nm)}">${escHtml(nm)}</td>
                  <td class="${latCls}">${escHtml(latTxt)}</td>
                  <td class="${flowCls}">${flowHtml}</td>
                </tr>`;
    }).join('');
    nodeTable.innerHTML = rows;
    {
      const keep = selectedName && prof.nodes.some(n => n.name === selectedName);
      if (!keep) {
        selectedName = prof.nodes[0]?.name || '—';
        const L = prof.nodes[0]?.lat;
        selectedLat = (L != null && L >= 0 && isFinite(L)) ? (L + ' ms') : '—';
      } else {
        const n = prof.nodes.find(x => x.name === selectedName);
        const L = n?.lat;
        if (L != null && L >= 0 && isFinite(L)) selectedLat = L + ' ms';
        // re-mark selected row (rows only marked first by default)
        const tr = [...nodeTable.querySelectorAll('tr')].find(r => r.dataset.name === selectedName);
        if (tr) {
          nodeTable.querySelectorAll('tr.selected').forEach(r => r.classList.remove('selected'));
          tr.classList.add('selected');
        }
      }
    }
    // keep session sort across group switch (do not reset sortKey)
    if (sortKey) {
      sortNodeTable(sortKey, sortDir);
      setSortHeader(sortKey, sortDir);
    } else {
      document.querySelectorAll('thead th.sortable').forEach(th => th.removeAttribute('aria-sort'));
    }
    // rebind selection anchor to live row (Shift-range after re-render)
    selectAnchorRow = nodeTable.querySelector('tr.selected')
      || [...nodeTable.querySelectorAll('tr')].find(r => r.dataset.name === selectedName)
      || null;
    // refresh hero only — no poll restart / traffic baseline reset
    if (typeof setConnected === 'function') setConnected(connected, { pin: false, sideEffects: false });
    else if (typeof refreshConnectedRow === 'function') refreshConnectedRow();
  }
  // single path: always setActiveGroup → nodes + 订阅设置同步
  document.querySelectorAll('#subSeg button, .seg button[data-sub]').forEach(btn => {
    btn.addEventListener('click', () => {
      const key = btn.dataset.sub || 'default';
      if (typeof setActiveGroup === 'function') setActiveGroup(key, { logIt: true });
      else {
        renderNodes(key);
        if (typeof syncSubUrlField === 'function') syncSubUrlField();
      }
    });
  });

  // boot: store catalog first (LS migrate); hide unwired routing/DNS for release UI
  if (typeof hideUnwiredSettings === 'function') hideUnwiredSettings();
  // Ordered boot: catalog hydrate before session restore.
  (async () => {
    try { await hydrateCatalogOnBoot(); } catch (_) {}
    try { await nexusBoot(); } catch (_) {}
  })();
  if (!_catalogUnloadBound) {
    _catalogUnloadBound = true;
    const flush = () => { if (typeof saveCatalog === 'function') saveCatalog(true); };
    window.addEventListener('pagehide', flush);
    window.addEventListener('beforeunload', flush);
  }



  function closeMenus() {
    document.querySelectorAll('.menu').forEach(m => m.classList.remove('open'));
    document.querySelectorAll('.menu-btn').forEach(b => {
      b.classList.remove('open');
      b.setAttribute('aria-expanded', 'false');
    });
    document.querySelectorAll('#testBtn').forEach(b => b.setAttribute('aria-expanded', 'false'));
    if (typeof closeSelectPops === 'function') closeSelectPops();
    if (typeof closeCtxMenu === 'function') closeCtxMenu();
    if (typeof closeLogCtxMenu === 'function') closeLogCtxMenu();
    if (typeof closeConnCtxMenu === 'function') closeConnCtxMenu();
      }
  function bindMenu(btnId, menuId) {
    const btn = document.getElementById(btnId);
    const menu = document.getElementById(menuId);
    if (!btn || !menu) return;
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const open = !menu.classList.contains('open');
      closeMenus();
      if (open) {
        menu.classList.add('open');
        btn.classList.add('open');
        btn.setAttribute('aria-expanded', 'true');
      }
    });
  }
  bindMenu('appMenuBtn', 'appMenu');
  bindMenu('testBtn', 'testMenu');
  document.addEventListener('click', () => closeMenus());
  document.querySelectorAll('.menu').forEach(m => m.addEventListener('click', e => e.stopPropagation()));

  
  // ——— import helpers (upstream: menu_add_from_clipboard / file / AsyncUpdate) ———
  function currentGid() {
    return (typeof activeGroupId === 'function') ? activeGroupId() : (document.querySelector('#subSeg button.active')?.dataset.sub || 'default');
  }
  function addNodesToCurrent(nodes, src) {
    if (!nodes || !nodes.length) {
      log('SYS', 'warn', t('log.importNone', { src: src || t('log.importSrc') }));
      return 0;
    }
    const gid = currentGid();
    const g = GROUPS.find(x => x.id === gid);
    const prof = ensureProfile(gid, g?.name, g?.url);
    if (!prof) return 0;
    const before = prof.nodes.length;
    // dedupe by addr|type
    const seen = new Set(prof.nodes.map(n => (n.addr || '') + '|' + (n.type || '')));
    let added = 0;
    for (const n of nodes) {
      const key = (n.addr || '') + '|' + (n.type || '');
      if (seen.has(key)) continue;
      seen.add(key);
      prof.nodes.push(n);
      added++;
    }
    if (g) g.count = prof.nodes.length;
    if (typeof renderNodes === 'function') renderNodes(gid);
    if (typeof saveCatalog === 'function') saveCatalog(true);
    log('SYS', 'ok', t('log.importedNExtra', { n: added, dedupe: added < nodes.length ? t('log.importDedupe', { n: nodes.length - added }) : '', src: src || '' }));
    return added;
  }
  async function importFromClipboard() {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (!text) { log('SYS', 'warn', t('log.clipEmpty')); return; }
      // if looks like subscription URL, AsyncUpdate(url)
      if (/^https?:\/\//i.test(text) && !text.includes('\n') && text.length < 500) {
        const gid = currentGid();
        const g = GROUPS.find(x => x.id === gid);
        if (g) {
          g.url = text.trim();
          if (typeof saveCatalog === 'function') saveCatalog(true);
          if (typeof syncSubUrlField === 'function') syncSubUrlField();
          log('SYS', 'info', t('log.clipIsSub'));
          await updateSubscription(gid);
          return;
        }
      }
      const nodes = await parseSubscriptionBodyAsync(text);
      addNodesToCurrent(nodes, t('js.clipboard'));
    } catch (_) {
      log('SYS', 'warn', t('log.clipFail'));
    }
  }
  function importFromFile() {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.txt,.json,.yaml,.yml,.conf,.sub,text/plain,application/json';
    input.style.display = 'none';
    document.body.appendChild(input);
    input.addEventListener('change', async () => {
      const f = input.files && input.files[0];
      input.remove();
      if (!f) return;
      try {
        const text = await f.text();
        if (/^https?:\/\//i.test(text.trim()) && text.trim().length < 500 && !text.includes('\n')) {
          const gid = currentGid();
          const g = GROUPS.find(x => x.id === gid);
          if (g) {
            g.url = text.trim();
            if (typeof saveCatalog === 'function') saveCatalog(true);
            log('SYS', 'info', t('log.fileIsSub'));
            await updateSubscription(gid);
            return;
          }
        }
        // share lines / JSON / clash — same path as sub update (keep credentials)
        let nodes = await parseSubscriptionBodyAsync(text);
        addNodesToCurrent(nodes, f.name);
      } catch (err) {
        log('SYS', 'warn', t('log.fileReadFail', { error: err && err.message || err }));
      }
    });
    input.click();
  }
  function importScanQr() {
    // opens camera/screen QR; here file image → no decode lib, guide user
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/*';
    input.style.display = 'none';
    document.body.appendChild(input);
    input.addEventListener('change', () => {
      const f = input.files && input.files[0];
      input.remove();
      if (!f) return;
      log('SYS', 'info', t('log.imagePicked', { name: f.name }));
    });
    input.click();
  }

  function handleMenuAct(act) {
    closeMenus();
    const map = {
      hide: async () => {
        try {
          const w = window.__TAURI__?.window?.getCurrentWindow?.();
          if (w) { await w.hide(); log('SYS', 'info', t('log.hiddenMenu')); return; }
        } catch (e) { log('SYS', 'warn', t('log.hideFail', { error: e })); }
        log('SYS', 'info', t('log.hiddenTray'));
      },
      show: async () => {
        try {
          const w = window.__TAURI__?.window?.getCurrentWindow?.();
          if (w) { await w.show(); await w.setFocus(); return; }
        } catch (_) {}
        log('SYS', 'info', t('log.showMain'));
      },
      'add-clip': () => { importFromClipboard(); },
      'add-file': () => { importFromFile(); },
      'scan-qr': () => { importScanQr(); },
      'dl-china': () => log('SYS', 'info', t('title.nyi')),
      'dl-iran': () => log('SYS', 'info', t('title.nyi')),
      'dl-russia': () => log('SYS', 'info', t('title.nyi')),
      'toggle-autostart': () => {
        const item = document.querySelector('[data-act="toggle-autostart"]');
        const on = !item.classList.contains('checked');
        item.classList.toggle('checked', on);
        item.querySelector('.check').textContent = on ? '✓' : '';
        log('SYS', 'info', on ? t('log.autostartOn') : t('log.autostartOff'));
      },
      'toggle-remember': () => { log('SYS', 'info', t('title.remember')); },
      'toggle-lan': () => { log('SYS', 'info', t('title.nyi')); },
      'proxy-on': () => {
        document.getElementById('sysToggle').checked = true;
        document.getElementById('sysChip').classList.add('on');
        nexusInvoke('set_system_proxy_cmd', { enabled: true }).then(r => {
          if (r.ok) log('SYS', 'info', String(r.data || t('log.proxyOn')));
          else if (!r.offline && r.error) log('SYS', 'warn', t('log.proxyErr', { error: r.error }));
          else log('SYS', 'info', t('log.proxyOn'));
        });
      },
      'proxy-off': () => {
        document.getElementById('sysToggle').checked = false;
        document.getElementById('sysChip').classList.remove('on');
        nexusInvoke('set_system_proxy_cmd', { enabled: false }).then(r => {
          if (r.ok) log('SYS', 'info', String(r.data || t('log.proxyOff')));
          else if (!r.offline && r.error) log('SYS', 'warn', t('log.proxyErr', { error: r.error }));
          else log('SYS', 'info', t('log.proxyOff'));
        });
      },
      'proxy-pac': () => log('SYS', 'info', t('log.proxyPac')),
      'tun-on': () => {
        if (typeof applyTun === 'function') applyTun(true);
        else {
          document.getElementById('tunToggle').checked = true;
          document.getElementById('tunChip').classList.add('on');
          log('SYS', 'info', t('log.tunNicOn'));
        }
      },
      'restart-core': () => log('CORE', 'warn', t('log.restartingCore')),
      'restart-app': () => log('SYS', 'warn', t('log.restartingApp')),
      connect: () => powerBtn.click(),
      'open-settings': () => {
        document.querySelectorAll('.sidebar .side-item').forEach(b => {
          b.classList.remove('active'); b.removeAttribute('aria-current');
        });
        const s = document.querySelector('.side-item[data-settings="basic"]');
        if (s) { s.classList.add('active'); s.setAttribute('aria-current', 'page'); }
        showView('settings', 'basic');
      },
      quit: () => { if (typeof requestQuitNexus === 'function') requestQuitNexus(); },
      'manage-groups': () => openGroupsDialog(),
      'runtime-stats': () => openStatsDialog(),
      'export-config': () => openExportDialog(),
    };
    (map[act] || (() => log('SYS', 'info', act)))();
  }
  document.querySelectorAll('.menu [data-act]').forEach(el => {
    el.addEventListener('click', () => handleMenuAct(el.dataset.act));
  });


  document.querySelectorAll('#view-settings .field[contenteditable="true"]').forEach(el => {
    el.addEventListener('input', () => markSettingsDirty());
  });
  // initial locale
  // Esc: close menus / dialogs (minimal a11y)
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    // topmost in-app ask first (sits above groups/edit)
    const ask = document.getElementById('askMask');
    if (ask && ask.classList.contains('open')) {
      if (typeof closeAsk === 'function') closeAsk(false);
      e.preventDefault();
      return;
    }
    const conf = document.getElementById('confirmMask');
    if (conf && conf.classList.contains('open')) {
      closeConfirm();
      pendingLeave = null;
      e.preventDefault();
      return;
    }
    for (const id of ['editMask', 'groupEditMask', 'groupsMask', 'qrMask', 'statsMask', 'exportMask', 'quitMask']) {
      const el = document.getElementById(id);
      if (el && el.classList.contains('open')) {
        closeDialog(id);
        e.preventDefault();
        return;
      }
    }
    if (typeof closeMenus === 'function') closeMenus();
  });

  loadUiPrefs();
  applyLocale(document.getElementById('langSelect')?.dataset.value || '简体中文');
  applyFont(document.getElementById('fontSelect')?.dataset.value || '系统默认');
  applyTheme(document.getElementById('themeSelect')?.dataset.value || 'System');
  applyIconStyle(document.getElementById('iconSelect')?.dataset.value || 'Monochrome');

  // --- Nexus Tauri bridge ---
  window.__NEXUS_BRIDGE__ = true;
  async function nexusBoot() {
    try {
      var ua = navigator.userAgent || '';
      var host = location.hostname || '';
      if (window.__TAURI_INTERNALS__ || window.__TAURI__ ||
          ua.indexOf('Tauri') !== -1 || location.protocol === 'tauri:' ||
          host === 'tauri.localhost' || (host.endsWith('.localhost') && host.indexOf('tauri') !== -1)) {
        document.documentElement.classList.add('desktop');
      }
    } catch (_) {}
    const id = await nexusInvoke('app_identity');
    if (id.ok) {
      log('SYS', 'info', t('log.backendId', { id: id.data?.identifier || 'Nexus', phase: id.data?.phase || '' }));
      if (id.data?.mixed_port) {
        window.__NEXUS_MIXED_PORT__ = id.data.mixed_port;
        document.querySelectorAll('.sb-c strong').forEach(el => {
          if (/:\d+$/.test(el.textContent) || el.textContent.includes('2080')) el.textContent = `127.0.0.1:${id.data.mixed_port}`;
        });
      }
    }
    else log('SYS', 'warn', t('log.backendOffline'));
    const st = await nexusInvoke('store_snapshot');
    if (st.ok) {
      const d = st.data || {};
      // Chips = spmode preference (store). Power = live tunnel (SESSION or orphan Core).
      if (typeof setChipOn === 'function') {
        setChipOn('tunToggle', 'tunChip', !!d.tun);
        setChipOn('sysToggle', 'sysChip', !!d.system_proxy);
      }
      const hideTrayBtn = document.getElementById('hideTraySwitch');
      if (hideTrayBtn) {
        const hide = !!d.hide_tray;
        hideTrayBtn.classList.toggle('on', hide);
        hideTrayBtn.setAttribute('aria-pressed', String(hide));
      }
      const cat = d.catalog;
      let n = 0;
      if (cat && cat.groups && typeof cat.groups === 'object') {
        for (const g of Object.values(cat.groups)) {
          if (g && Array.isArray(g.nodes)) n += g.nodes.length;
        }
      }
      log('SYS', 'info', t('log.localCfg', { n, tun: d.tun ? t('log.on') : t('log.off'), proxy: d.system_proxy ? t('log.on') : t('log.off') }));
    }
    // Power sync: residual Core/mixed/utun after quit/crash → paint 已连接 (not dead chrome).
    try {
      const ss = await nexusInvoke('session_status');
      const d = ss && ss.ok ? ss.data : null;
      // 4A: trust server-side running (already tightened); do not OR mixed alone.
      const live = !!(d && d.running);
      if (live) {
        if (typeof setChipOn === 'function') {
          if (d.tun != null) setChipOn('tunToggle', 'tunChip', !!d.tun);
          if (d.system_proxy != null) setChipOn('sysToggle', 'sysChip', !!d.system_proxy);
        }
        let pinName = '';
        try { pinName = localStorage.getItem('nexus.lastConnected') || ''; } catch (_) {}
        if (pinName && typeof selectedName !== 'undefined') {
          selectedName = pinName;
          connectedName = pinName;
          const row = nodeTable && [...nodeTable.querySelectorAll('tr')].find(r => r.dataset.name === pinName);
          if (row) {
            nodeTable.querySelectorAll('tr.selected').forEach(r => r.classList.remove('selected'));
            row.classList.add('selected');
          }
        }
        if (typeof setConnected === 'function') setConnected(true, { pin: !!pinName });
        const bits = [];
        if (d.process_alive) bits.push(t('stats.coreProc'));
        if (d.mixed_open) bits.push('mixed:' + mixedPort());
        if (d.rpc_running) bits.push('RPC');
        log('OK', 'ok', t('log.syncState', { bits: bits.join(' · ') || t('log.tunnelStill') }));
      }
    } catch (_) {}
    if (typeof refreshTrayMenu === 'function') refreshTrayMenu();
  }

  // Policy A: quit = full teardown; warn when tunnel may still be live.
  async function requestQuitNexus() {
    let live = !!connected;
    if (!live) {
      try {
        const ss = await nexusInvoke('session_status');
        const d = ss && ss.ok ? ss.data : null;
        live = !!(d && d.running);
      } catch (_) {}
    }
    if (live) {
      const mask = document.getElementById('quitMask');
      if (mask) {
        mask.hidden = false;
        mask.classList.add('open');
        return;
      }
    }
    await performQuitNexus();
  }
  async function performQuitNexus() {
    const mask = document.getElementById('quitMask');
    if (mask) { mask.classList.remove('open'); mask.hidden = true; }
    // Paint power OFF before process dies (user: 主动处理连接按钮).
    if (typeof setConnected === 'function') setConnected(false);
    try { localStorage.removeItem('nexus.lastConnected'); } catch (_) {}
    if (typeof stopConnPoll === 'function') stopConnPoll();
    log('SYS', 'warn', t('log.quitting'));
    const r = await nexusInvoke('app_quit', { force: true });
    if (r && !r.ok && !r.offline) log('SYS', 'warn', t('log.quitFail', { error: r.error || '' }));
    else if (r && r.offline) log('SYS', 'warn', t('log.quitOffline'));
  }
  document.getElementById('quitCancel')?.addEventListener('click', () => {
    const mask = document.getElementById('quitMask');
    if (mask) { mask.classList.remove('open'); mask.hidden = true; }
  });
  document.getElementById('quitConfirm')?.addEventListener('click', () => { performQuitNexus(); });
  document.getElementById('quitMask')?.addEventListener('click', (e) => {
    if (e.target.id === 'quitMask') {
      const mask = document.getElementById('quitMask');
      if (mask) { mask.classList.remove('open'); mask.hidden = true; }
    }
  });
  const _applyTun = applyTun;
  applyTun = function(on, opts) {
    _applyTun(on, opts);
    // set_spmode_vpn: elevate (osascript password sheet) + re-Start if tunnel live.
    nexusInvoke('set_tun_cmd', { enabled: !!on }).then(async (r) => {
      if (!r || r.offline) return;
      if (!r.ok) {
        // elevate cancel / fail → revert chip
        setChipOn('tunToggle', 'tunChip', !on);
        log('SYS', 'warn', r.error || t('log.tunSwitchFail'));
        return;
      }
      const d = r.data;
      const note = (d && typeof d === 'object' && d.note) ? d.note : String(d || '');
      if (note) log('SYS', 'info', note);
      // Live tunnel must re-Start so generate includes/excludes tun-in.
      if (typeof connected !== 'undefined' && connected) {
        const name = (typeof connectedName !== 'undefined' && connectedName) || selectedName;
        const payload = (typeof nodeConnectPayload === 'function') ? nodeConnectPayload(name) : null;
        if (!payload) {
          log('SYS', 'warn', t('log.tunNoCfg'));
          return;
        }
        await runSessionOp('reconnect', async () => {
          log('CORE', 'info', on ? t('log.tunRestartOn') : t('log.tunRestartOff'));
          const cr = await connectSelectedWithHelper(Object.assign({
            profile_id: 1,
            tun: !!on,
            system_proxy: !!document.getElementById('sysToggle')?.checked,
          }, payload));
          if (!cr || cr.offline) return;
          if (!cr.ok) {
            log('CORE', 'warn', t('log.tunRestartFail', { error: cr.error }));
            return;
          }
          if (cr.data?.start_error) {
            log('CORE', 'warn', `Tun re-Start: ${cr.data.start_error}`);
            try { await nexusInvoke('disconnect_selected'); } catch (_) {}
            if (typeof refreshFirewall === 'function') refreshFirewall();
            return;
          }
          if (cr.data?.started) {
            // Tun recreate: re-baseline Core counters only (intentional).
            _coreBaseUp = null;
            _coreBaseDown = null;
            log('OK', 'ok', on ? t('log.tunApplied') : t('log.tunBackProxy'));
            if (cr.data?.proxy_note) log('SYS', 'info', cr.data.proxy_note);
          }
        });
      }
    });
  };
  document.getElementById('sysToggle')?.addEventListener('change', (e) => {
    const on = !!e.target.checked;
    document.getElementById('sysChip')?.classList.toggle('on', on);
    nexusInvoke('set_system_proxy_cmd', { enabled: on }).then(r => {
      if (r.ok) log('SYS', 'info', String(r.data));
      else if (!r.offline && r.error) log('SYS', 'warn', t('log.proxyErr', { error: r.error }));
      if (typeof refreshTrayMenu === 'function') refreshTrayMenu();
    });
  });
  syncSubUrlField();

  if (typeof bindSideNav === 'function') bindSideNav();
  showView('home');
