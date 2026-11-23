// Mounted once per panel, including panels rendered inside a shadow root
{
  const header = shellEl.querySelector('.session-right') || shellEl.querySelector('header');
  const settings = shellEl.querySelector('.cloud-actions-row') || shellEl.querySelector('.settings-view-root');
  function targetForSession() {
    const state = telemetryStore.getState();
    const telemetry = state.telemetry;
    const active = telemetry?.activeConversation;
    const id = state.selectedConversationId || active?.cascadeId;
    const row = telemetry?.conversations?.find(item => item.cascadeId === id) ||
      (active?.cascadeId === id ? active : null);
    const provider = { cli: 'agy', standalone: 'antigravity' }[row?.appSource];
    if ((row?.appSource && !provider) || telemetry?.endpoint?.isRemote ||
        !/^[a-f\d]{8}(?:-[a-f\d]{4}){3}-[a-f\d]{12}$/i.test(id || '')) return 'antipower://plugins';
    const profile = provider === 'agy' && row.profile ? '?profile=' + encodeURIComponent(row.profile) : '';
    return 'antipower://session/' + (provider ? provider + '/' : '') + id + profile;
  }
  function addLink(parent, session) {
    if (!parent) return;
    const link = document.createElement('a');
    link.className = 'btn-secondary';
    link.textContent = 'Anti-Power';
    function sync() {
      const url = session ? targetForSession() : 'antipower://plugins';
      link.setAttribute('href', url);
      const bridge = getWindowBridgeConfig();
      link.hidden = Boolean(bridge?.endpointId && bridge.endpointId !== 'local');
    }
    sync();
    onCleanup(telemetryStore.subscribe(st => [st.telemetry, st.selectedConversationId], sync, { immediate: false }));
    link.addEventListener('click', async event => {
      sync();
      if (!window.sidecar?.fetch && !getWindowBridgeConfig()) return;
      event.preventDefault();
      try {
        await callSidecarApi('/api/open-anti-power', { method: 'POST', body: { url: link.getAttribute('href') } });
        link.textContent = 'Anti-Power';
      } catch (err) {
        link.textContent = 'Anti-Power · ' + err.message;
      }
    });
    if (session) parent.insertBefore(link, parent.firstChild);
    else parent.appendChild(link);
  }
  addLink(header, true);
  addLink(settings, false);
}
