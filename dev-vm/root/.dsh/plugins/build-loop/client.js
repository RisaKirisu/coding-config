window.__ModuleLoader__.load({
  id: '@devvm/dsh-build-loop',
  factory: (require) => {
    var module = { exports: {} };
    const React = require('react');

    const CSS = `
      .bl-page{padding:24px 32px;max-width:920px;color:var(--dsw-alias-label-primary,#1c1c1e);font-family:system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;box-sizing:border-box}
      .bl-title{font-size:20px;font-weight:700;margin:0 0 6px}
      .bl-subtitle{font-size:14px;line-height:1.55;margin:0 0 22px;color:var(--dsw-alias-label-secondary,#6d6d72)}
      .bl-card{padding:20px;border:1px solid var(--dsw-alias-border-l1,rgba(125,125,125,.2));border-radius:14px;background:var(--dsw-alias-bg-layer-1,rgba(125,125,125,.07));margin-bottom:16px}
      .bl-field{margin-bottom:17px}.bl-field:last-child{margin-bottom:0}
      .bl-label{display:flex;justify-content:space-between;align-items:baseline;margin:0 0 7px;font-size:13px;font-weight:650}
      .bl-hint{font-size:12px;font-weight:400;color:var(--dsw-alias-label-secondary,#6d6d72)}
      .bl-input,.bl-textarea{width:100%;padding:9px 13px;border:1px solid var(--dsw-alias-border-l2,rgba(125,125,125,.3));border-radius:9px;background:var(--dsw-alias-bg-layer-2,rgba(125,125,125,.1));color:var(--dsw-alias-label-primary,#1c1c1e);font-size:13px;outline:none;box-sizing:border-box;color-scheme:light dark}
      .bl-textarea{min-height:260px;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;line-height:1.45;resize:vertical;white-space:pre}
      .bl-input:focus,.bl-textarea:focus{border-color:#007aff;box-shadow:0 0 0 2px rgba(0,122,255,.22)}
      .bl-row{display:grid;grid-template-columns:1fr 1fr;gap:16px}
      .bl-actions{display:flex;align-items:center;gap:12px;margin-top:8px;flex-wrap:wrap}
      .bl-btn{height:38px;padding:0 20px;border:0;border-radius:9px;font-size:14px;font-weight:650;white-space:nowrap;cursor:pointer}
      .bl-primary{background:#007aff;color:#fff;box-shadow:0 1px 3px rgba(0,0,0,.2)}.bl-primary:hover{background:#0066d6}
      .bl-secondary{background:var(--dsw-alias-bg-layer-2,rgba(125,125,125,.15));color:var(--dsw-alias-label-primary,#1c1c1e)}
      .bl-link{background:none;border:0;padding:0;height:auto;font-size:12px;color:#007aff;cursor:pointer}
      .bl-btn:disabled{opacity:.55;cursor:default}
      .bl-notice{font-size:13px;color:var(--dsw-alias-state-success-primary,#22a447)}
      .bl-error{font-size:13px;color:var(--dsw-alias-state-error-primary,#d84040)}
      .bl-flow{font-size:13px;line-height:1.6;margin:0;padding-left:18px;color:var(--dsw-alias-label-secondary,#6d6d72)}
      .bl-flow code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--dsw-alias-label-primary,#1c1c1e)}
    `;

    const PERSONAS = [
      { key: 'buildPersona', label: 'Build agent prompt', hint: 'implements the ticket; receives fix rounds' },
      { key: 'reviewPersona', label: 'Review agent prompt', hint: 'spec + standards + simplicity; reports only' },
      { key: 'testPersona', label: 'Test agent prompt', hint: 'behavior vs implementation, no hand-rolled mocks, mutation tests' },
    ];

    function Field(props) {
      return React.createElement(
        'div',
        { className: 'bl-field' },
        React.createElement(
          'div',
          { className: 'bl-label' },
          React.createElement('span', null, props.label, props.hint ? React.createElement('span', { className: 'bl-hint' }, ' — ' + props.hint) : null),
          props.onReset ? React.createElement('button', { className: 'bl-link', type: 'button', onClick: props.onReset }, props.modified ? 'reset to default' : 'default') : null,
        ),
        props.children,
      );
    }

    function SettingsPage() {
      const [state, setState] = React.useState({ loading: true, saving: false, config: null, defaults: null, notice: '', error: '' });

      const load = (init) => fetch('/api/build-loop/config', init).then(async (response) => {
        const value = await response.json();
        if (!response.ok) throw new Error(value.error || String(response.status));
        return value;
      });

      React.useEffect(() => {
        let active = true;
        load().then((value) => {
          if (active) setState((current) => ({ ...current, loading: false, config: value.config, defaults: value.defaults }));
        }).catch((error) => {
          if (active) setState((current) => ({ ...current, loading: false, error: 'Load failed: ' + error.message }));
        });
        return () => { active = false; };
      }, []);

      const patch = (key, value) => setState((current) => ({ ...current, notice: '', error: '', config: { ...current.config, [key]: value } }));

      const commit = async (init, doneNotice) => {
        setState((current) => ({ ...current, saving: true, notice: '', error: '' }));
        try {
          const value = await load(init);
          setState((current) => ({ ...current, saving: false, config: value.config, defaults: value.defaults, notice: doneNotice }));
          setTimeout(() => setState((current) => ({ ...current, notice: '' })), 3000);
        } catch (error) {
          setState((current) => ({ ...current, saving: false, error: 'Save failed: ' + error.message }));
        }
      };

      const save = () => commit({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(state.config),
      }, 'Saved. Applies to the next build_ticket call.');

      const resetAll = () => {
        if (!window.confirm('Discard every override and return all prompts and flow settings to defaults?')) return;
        commit({ method: 'DELETE' }, 'All settings reset to defaults.');
      };

      if (state.loading) return React.createElement('div', { className: 'bl-page' }, React.createElement('style', null, CSS), 'Loading…');
      if (!state.config) return React.createElement('div', { className: 'bl-page' }, React.createElement('style', null, CSS), React.createElement('span', { className: 'bl-error' }, state.error));

      const c = state.config;
      const d = state.defaults;

      return React.createElement(
        'div',
        { className: 'bl-page' },
        React.createElement('style', null, CSS),
        React.createElement('h2', { className: 'bl-title' }, 'Build Loop'),
        React.createElement('p', { className: 'bl-subtitle' }, 'Prompts and flow for the build_ticket tool: build → review ‖ test → fix, repeated until clean or the fix budget is spent. Saved values are read at the start of each build_ticket call.'),
        React.createElement(
          'div',
          { className: 'bl-card' },
          React.createElement('ol', { className: 'bl-flow' },
            React.createElement('li', null, 'Build agent implements the ticket and returns its report.'),
            React.createElement('li', null, 'Review agent and test agent audit in parallel, each returning ', React.createElement('code', null, '{clean, findings, report}'), '.'),
            React.createElement('li', null, 'Both clean → all three reports return to the orchestrator. Otherwise findings go back to the same build agent as a fix round.'),
            React.createElement('li', null, 'After ', React.createElement('code', null, String(c.maxFixRounds)), ' fix round(s) the current state is reported as unresolved.'),
          ),
        ),
        React.createElement(
          'div',
          { className: 'bl-card' },
          React.createElement(
            'div',
            { className: 'bl-row' },
            React.createElement(Field, { label: 'Max fix rounds', hint: '0 = audit once, never fix', modified: c.maxFixRounds !== d.maxFixRounds, onReset: () => patch('maxFixRounds', d.maxFixRounds) },
              React.createElement('input', { className: 'bl-input', type: 'number', min: 0, step: 1, value: c.maxFixRounds, onChange: (event) => patch('maxFixRounds', Math.max(0, Math.floor(Number(event.target.value) || 0))) })),
            React.createElement(Field, { label: 'Subagent provider', hint: 'must support persona + structured output (spawn)', modified: c.provider !== d.provider, onReset: () => patch('provider', d.provider) },
              React.createElement('input', { className: 'bl-input', type: 'text', value: c.provider, onChange: (event) => patch('provider', event.target.value) })),
          ),
          React.createElement(Field, { label: 'Tools denied to every child', hint: 'one name per line; names the child cannot see are ignored', modified: c.deniedTools.join('\n') !== d.deniedTools.join('\n'), onReset: () => patch('deniedTools', d.deniedTools) },
            React.createElement('textarea', { className: 'bl-textarea', style: { minHeight: '120px' }, value: c.deniedTools.join('\n'), onChange: (event) => patch('deniedTools', event.target.value.split('\n').map((line) => line.trim()).filter(Boolean)) })),
        ),
        ...PERSONAS.map((persona) => React.createElement(
          'div',
          { className: 'bl-card', key: persona.key },
          React.createElement(Field, { label: persona.label, hint: persona.hint, modified: c[persona.key] !== d[persona.key], onReset: () => patch(persona.key, d[persona.key]) },
            React.createElement('textarea', { className: 'bl-textarea', value: c[persona.key], spellCheck: false, onChange: (event) => patch(persona.key, event.target.value) })),
        )),
        React.createElement(
          'div',
          { className: 'bl-actions' },
          React.createElement('button', { className: 'bl-btn bl-primary', type: 'button', disabled: state.saving, onClick: save }, 'Save'),
          React.createElement('button', { className: 'bl-btn bl-secondary', type: 'button', disabled: state.saving, onClick: resetAll }, 'Reset all to defaults'),
          state.notice ? React.createElement('span', { className: 'bl-notice' }, state.notice) : null,
          state.error ? React.createElement('span', { className: 'bl-error' }, state.error) : null,
        ),
      );
    }

    module.exports = {
      name: '@devvm/dsh-build-loop',
      inject: ['slots'],
      apply(ctx) {
        const slots = ctx.get('slots');
        if (!slots) return;
        slots.inject('settings.section', () => slots.register(
          { name: 'settings.section', id: 'build-loop-config', order: 15, label: () => 'Build Loop' },
          () => React.createElement(SettingsPage, null),
        ));
      },
    };

    return module.exports;
  },
});
