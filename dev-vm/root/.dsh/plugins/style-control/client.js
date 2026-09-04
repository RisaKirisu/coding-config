window.__ModuleLoader__.load({
  id: '@devvm/dsh-style-control',
  factory: (require) => {
    const React = require('react');

    const STYLE_TAG_ID = 'devvm-dsh-style-control-styles';
    const CSS = `
      .sc-page {
        padding: 24px 32px;
        max-width: 840px;
        color: var(--dsw-alias-label-primary, #1c1c1e);
        font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        box-sizing: border-box;
      }
      .sc-title {
        font-size: 20px;
        font-weight: 700;
        margin: 0 0 6px;
        color: var(--dsw-alias-label-primary, #1c1c1e);
      }
      .sc-subtitle {
        font-size: 14px;
        line-height: 1.55;
        margin: 0 0 22px;
        color: var(--dsw-alias-label-secondary, #6d6d72);
      }
      .sc-card {
        padding: 20px;
        border: 1px solid var(--dsw-alias-border-l1, rgba(125,125,125,.2));
        border-radius: 14px;
        background: var(--dsw-alias-bg-layer-1, rgba(125,125,125,.07));
        margin-bottom: 16px;
      }
      .sc-field {
        margin-bottom: 16px;
      }
      .sc-field:last-child {
        margin-bottom: 0;
      }
      .sc-label {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        margin: 0 0 7px;
        font-size: 13px;
        font-weight: 650;
      }
      .sc-hint {
        font-size: 12px;
        font-weight: 400;
        color: var(--dsw-alias-label-secondary, #6d6d72);
      }
      .sc-select-wrap {
        position: relative;
        display: flex;
        align-items: center;
        width: 100%;
      }
      .sc-select {
        width: 100%;
        height: 38px;
        padding: 0 32px 0 12px;
        appearance: none;
        -webkit-appearance: none;
        border: 1px solid var(--dsw-alias-border-l2, rgba(125,125,125,.3));
        border-radius: 9px;
        background: var(--dsw-alias-bg-layer-2, rgba(125,125,125,.1));
        color: var(--dsw-alias-label-primary, #1c1c1e);
        font-size: 13px;
        outline: none;
        box-sizing: border-box;
        color-scheme: light dark;
      }
      .sc-select:focus, .sc-input:focus, .sc-textarea:focus {
        border-color: #007aff;
        box-shadow: 0 0 0 2px rgba(0,122,255,.22);
      }
      .sc-arrow {
        position: absolute;
        right: 12px;
        pointer-events: none;
        color: var(--dsw-alias-label-secondary, #6d6d72);
        font-size: 11px;
      }
      .sc-input, .sc-textarea {
        width: 100%;
        padding: 9px 12px;
        border: 1px solid var(--dsw-alias-border-l2, rgba(125,125,125,.3));
        border-radius: 9px;
        background: var(--dsw-alias-bg-layer-2, rgba(125,125,125,.1));
        color: var(--dsw-alias-label-primary, #1c1c1e);
        font-size: 13px;
        outline: none;
        box-sizing: border-box;
        color-scheme: light dark;
      }
      .sc-textarea {
        min-height: 220px;
        font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
        line-height: 1.45;
        resize: vertical;
        white-space: pre-wrap;
      }
      .sc-row {
        display: flex;
        gap: 12px;
        align-items: center;
      }
      .sc-actions {
        display: flex;
        align-items: center;
        gap: 12px;
        margin-top: 18px;
        flex-wrap: wrap;
      }
      .sc-btn {
        height: 36px;
        padding: 0 18px;
        border: 0;
        border-radius: 9px;
        font-size: 13px;
        font-weight: 600;
        white-space: nowrap;
        cursor: pointer;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 6px;
      }
      .sc-btn-primary {
        background: #007aff;
        color: #fff;
        box-shadow: 0 1px 3px rgba(0,0,0,.2);
      }
      .sc-btn-primary:hover {
        background: #0066d6;
      }
      .sc-btn-primary:disabled {
        opacity: .55;
        cursor: default;
      }
      .sc-btn-secondary {
        background: var(--dsw-alias-bg-layer-2, rgba(125,125,125,.15));
        color: var(--dsw-alias-label-primary, #1c1c1e);
        border: 1px solid var(--dsw-alias-border-l1, rgba(125,125,125,.2));
      }
      .sc-btn-secondary:hover {
        background: var(--dsw-alias-interactive-bg-hover, rgba(125,125,125,.25));
      }
      .sc-btn-danger {
        background: rgba(216,64,64,.12);
        color: var(--dsw-alias-state-error-primary, #d84040);
        border: 1px solid rgba(216,64,64,.3);
      }
      .sc-btn-danger:hover {
        background: rgba(216,64,64,.22);
      }
      .sc-badge {
        display: inline-block;
        padding: 3px 8px;
        border-radius: 6px;
        font-size: 11px;
        font-weight: 600;
        background: rgba(0,122,255,.15);
        color: #007aff;
      }
      .sc-notice {
        font-size: 13px;
        color: var(--dsw-alias-state-success-primary, #22a447);
        font-weight: 500;
      }
      .sc-error {
        font-size: 13px;
        color: var(--dsw-alias-state-error-primary, #d84040);
      }
      .sc-inline-form {
        display: flex;
        gap: 8px;
        align-items: center;
        margin-top: 10px;
        padding: 10px 14px;
        background: var(--dsw-alias-bg-layer-2, rgba(125,125,125,.1));
        border-radius: 9px;
      }
    `;

    function injectStyleSheet() {
      if (typeof document === 'undefined') return;
      if (document.getElementById(STYLE_TAG_ID)) return;
      const tag = document.createElement('style');
      tag.id = STYLE_TAG_ID;
      tag.textContent = CSS;
      document.head.appendChild(tag);
    }

    function StyleSettingsPage() {
      injectStyleSheet();
      const [state, setState] = React.useState({
        loading: true,
        saving: false,
        presets: [],
        activePresetId: 'default',
        selectedEditId: 'default',
        isAdding: false,
        confirmDelete: false,
        confirmReset: false,
        newPresetName: '',
        notice: '',
        error: '',
      });

      const load = async () => {
        try {
          const res = await fetch('/api/style-control/presets');
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          const data = await res.json();
          setState((curr) => {
            const presets = data.presets || [];
            const activePresetId = data.activePresetId || (presets[0] && presets[0].id) || 'default';
            const selectedEditId = presets.some((p) => p.id === curr.selectedEditId)
              ? curr.selectedEditId
              : activePresetId;
            return {
              ...curr,
              loading: false,
              presets,
              activePresetId,
              selectedEditId,
              error: '',
            };
          });
        } catch (err) {
          setState((curr) => ({
            ...curr,
            loading: false,
            error: 'Failed to load presets: ' + (err.message || String(err)),
          }));
        }
      };

      React.useEffect(() => {
        load();
      }, []);

      const currentPreset = state.presets.find((p) => p.id === state.selectedEditId) || state.presets[0];

      const handlePresetChange = (e) => {
        setState((curr) => ({
          ...curr,
          selectedEditId: e.target.value,
          isAdding: false,
          confirmDelete: false,
          notice: '',
          error: '',
        }));
      };

      const updateCurrentPreset = (field, val) => {
        setState((curr) => ({
          ...curr,
          notice: '',
          error: '',
          presets: curr.presets.map((p) =>
            p.id === curr.selectedEditId ? { ...p, [field]: val } : p
          ),
        }));
      };

      const handleAddPreset = () => {
        const name = state.newPresetName.trim();
        if (!name) return;
        const id = name.toLowerCase().replace(/[^a-z0-9]+/g, '-') + '-' + Date.now().toString(36);
        const newPreset = {
          id,
          name,
          content: 'Maintain clear and focused formatting.',
        };
        setState((curr) => ({
          ...curr,
          presets: [...curr.presets, newPreset],
          selectedEditId: id,
          isAdding: false,
          newPresetName: '',
          notice: `Created preset "${name}". Click Save to persist.`,
        }));
      };

      const handleDeletePreset = () => {
        if (state.presets.length <= 1) {
          setState((curr) => ({ ...curr, error: 'Cannot delete the only remaining preset.' }));
          return;
        }
        const remaining = state.presets.filter((p) => p.id !== state.selectedEditId);
        const nextSelected = remaining[0].id;
        const nextActive = state.activePresetId === state.selectedEditId ? nextSelected : state.activePresetId;

        setState((curr) => ({
          ...curr,
          presets: remaining,
          selectedEditId: nextSelected,
          activePresetId: nextActive,
          confirmDelete: false,
          notice: 'Preset deleted. Click Save to persist.',
        }));
      };

      const handleSetDefault = () => {
        setState((curr) => ({
          ...curr,
          activePresetId: curr.selectedEditId,
          notice: `Set "${currentPreset.name}" as default preset.`,
        }));
      };

      const handleSave = async () => {
        setState((curr) => ({ ...curr, saving: true, notice: '', error: '' }));
        try {
          const res = await fetch('/api/style-control/presets', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              presets: state.presets,
              activePresetId: state.activePresetId,
            }),
          });
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          const data = await res.json();
          setState((curr) => ({
            ...curr,
            saving: false,
            presets: data.presets,
            activePresetId: data.activePresetId,
            notice: 'Style presets saved successfully!',
          }));
          setTimeout(() => {
            setState((curr) => ({ ...curr, notice: '' }));
          }, 3000);
        } catch (err) {
          setState((curr) => ({
            ...curr,
            saving: false,
            error: 'Failed to save: ' + (err.message || String(err)),
          }));
        }
      };

      const handleResetAll = async () => {
        setState((curr) => ({ ...curr, saving: true, notice: '', error: '' }));
        try {
          const res = await fetch('/api/style-control/reset', { method: 'POST' });
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          const data = await res.json();
          setState((curr) => ({
            ...curr,
            saving: false,
            presets: data.presets,
            activePresetId: data.activePresetId,
            selectedEditId: data.activePresetId,
            confirmReset: false,
            notice: 'Reset to default style presets.',
          }));
        } catch (err) {
          setState((curr) => ({
            ...curr,
            saving: false,
            error: 'Reset failed: ' + (err.message || String(err)),
          }));
        }
      };

      if (state.loading) {
        return React.createElement('div', { className: 'sc-page' },
          React.createElement('div', { className: 'sc-subtitle' }, 'Loading style presets…')
        );
      }

      return React.createElement(
        'div',
        { className: 'sc-page' },
        React.createElement('h2', { className: 'sc-title' }, 'Style Control'),
        React.createElement(
          'p',
          { className: 'sc-subtitle' },
          'Create and edit agent formatting and tone presets. The active preset text is injected into the agent system prompt at order 1 inside <formatting_and_tone> tags.'
        ),

        React.createElement(
          'div',
          { className: 'sc-card' },
          React.createElement(
            'div',
            { className: 'sc-field' },
            React.createElement(
              'div',
              { className: 'sc-label' },
              React.createElement('span', null, 'Preset to Edit'),
              currentPreset && currentPreset.id === state.activePresetId
                ? React.createElement('span', { className: 'sc-badge' }, 'Active Default')
                : null
            ),
            React.createElement(
              'div',
              { className: 'sc-row' },
              React.createElement(
                'div',
                { className: 'sc-select-wrap', style: { flex: 1 } },
                React.createElement(
                  'select',
                  {
                    className: 'sc-select',
                    value: state.selectedEditId,
                    onChange: handlePresetChange,
                  },
                  state.presets.map((p) =>
                    React.createElement(
                      'option',
                      { key: p.id, value: p.id },
                      p.id === state.activePresetId ? `★ ${p.name} (Default)` : p.name
                    )
                  )
                ),
                React.createElement('span', { className: 'sc-arrow' }, '▼')
              ),
              React.createElement(
                'button',
                {
                  type: 'button',
                  className: 'sc-btn sc-btn-secondary',
                  onClick: () => setState((curr) => ({ ...curr, isAdding: !curr.isAdding, newPresetName: '' })),
                },
                state.isAdding ? 'Cancel' : '+ Add Preset'
              )
            ),
            state.isAdding && React.createElement(
              'div',
              { className: 'sc-inline-form' },
              React.createElement('input', {
                type: 'text',
                className: 'sc-input',
                placeholder: 'New preset name (e.g. Concise, Academic)',
                value: state.newPresetName,
                onChange: (e) => setState((curr) => ({ ...curr, newPresetName: e.target.value })),
                onKeyDown: (e) => { if (e.key === 'Enter') handleAddPreset(); },
              }),
              React.createElement(
                'button',
                {
                  type: 'button',
                  className: 'sc-btn sc-btn-primary',
                  onClick: handleAddPreset,
                  disabled: !state.newPresetName.trim(),
                },
                'Create'
              )
            )
          ),

          currentPreset && React.createElement(
            React.Fragment,
            null,
            React.createElement(
              'div',
              { className: 'sc-field' },
              React.createElement('div', { className: 'sc-label' }, React.createElement('span', null, 'Preset Display Name')),
              React.createElement('input', {
                type: 'text',
                className: 'sc-input',
                value: currentPreset.name || '',
                onChange: (e) => updateCurrentPreset('name', e.target.value),
              })
            ),
            React.createElement(
              'div',
              { className: 'sc-field' },
              React.createElement(
                'div',
                { className: 'sc-label' },
                React.createElement('span', null, 'Formatting & Tone Instructions'),
                React.createElement('span', { className: 'sc-hint' }, 'Injected inside <formatting_and_tone> tag')
              ),
              React.createElement('textarea', {
                className: 'sc-textarea',
                value: currentPreset.content || '',
                placeholder: 'Enter style instructions for the agent (e.g. tone, structure, brevity, perspective)...',
                onChange: (e) => updateCurrentPreset('content', e.target.value),
              })
            ),
            React.createElement(
              'div',
              { className: 'sc-row', style: { justifyContent: 'space-between', marginTop: '12px' } },
              React.createElement(
                'button',
                {
                  type: 'button',
                  className: 'sc-btn sc-btn-secondary',
                  onClick: handleSetDefault,
                  disabled: currentPreset.id === state.activePresetId,
                },
                currentPreset.id === state.activePresetId ? '✓ Already Active Default' : 'Set as Active Default'
              ),
              state.presets.length > 1 && (
                state.confirmDelete
                  ? React.createElement(
                      'div',
                      { className: 'sc-row', style: { gap: '6px' } },
                      React.createElement('span', { style: { fontSize: '12px', color: '#d84040' } }, 'Confirm delete?'),
                      React.createElement('button', { type: 'button', className: 'sc-btn sc-btn-danger', onClick: handleDeletePreset }, 'Yes, Delete'),
                      React.createElement('button', { type: 'button', className: 'sc-btn sc-btn-secondary', onClick: () => setState((c) => ({ ...c, confirmDelete: false })) }, 'Cancel')
                    )
                  : React.createElement(
                      'button',
                      {
                        type: 'button',
                        className: 'sc-btn sc-btn-danger',
                        onClick: () => setState((c) => ({ ...c, confirmDelete: true })),
                      },
                      'Delete Preset'
                    )
              )
            )
          )
        ),

        React.createElement(
          'div',
          { className: 'sc-actions' },
          React.createElement(
            'button',
            {
              type: 'button',
              className: 'sc-btn sc-btn-primary',
              onClick: handleSave,
              disabled: state.saving,
            },
            state.saving ? 'Saving…' : 'Save Changes'
          ),
          state.confirmReset
            ? React.createElement(
                'div',
                { className: 'sc-row', style: { gap: '6px' } },
                React.createElement('span', { style: { fontSize: '12px', color: '#d84040' } }, 'Reset all to default presets?'),
                React.createElement('button', { type: 'button', className: 'sc-btn sc-btn-danger', onClick: handleResetAll }, 'Confirm Reset'),
                React.createElement('button', { type: 'button', className: 'sc-btn sc-btn-secondary', onClick: () => setState((c) => ({ ...c, confirmReset: false })) }, 'Cancel')
              )
            : React.createElement(
                'button',
                {
                  type: 'button',
                  className: 'sc-btn sc-btn-secondary',
                  onClick: () => setState((c) => ({ ...c, confirmReset: true })),
                  disabled: state.saving,
                },
                'Reset to Defaults'
              ),
          state.notice ? React.createElement('span', { className: 'sc-notice' }, state.notice) : null,
          state.error ? React.createElement('span', { className: 'sc-error' }, state.error) : null
        )
      );
    }

    /**
     * StyleChatDropdown renders into 'conversation.input.right'.
     * Styled strictly with explicit inline styles so it matches ModelSelect
     * and cannot be overridden or broken by missing stylesheets.
     */
    function StyleChatDropdown(props) {
      const sessionId = props?.session?.id;
      const [data, setData] = React.useState(null);
      const [open, setOpen] = React.useState(false);
      const [hovered, setHovered] = React.useState(false);
      const [hoveredPresetId, setHoveredPresetId] = React.useState(null);
      const rootRef = React.useRef(null);

      React.useEffect(() => {
        let active = true;
        fetch('/api/style-control/presets')
          .then((res) => (res.ok ? res.json() : null))
          .then((val) => {
            if (active && val) setData(val);
          })
          .catch(() => {});
        return () => {
          active = false;
        };
      }, [sessionId]);

      React.useEffect(() => {
        if (!open) return;
        const closeOutside = (e) => {
          if (rootRef.current && !rootRef.current.contains(e.target)) {
            setOpen(false);
          }
        };
        const onKeyDown = (e) => {
          if (e.key === 'Escape') setOpen(false);
        };
        document.addEventListener('mousedown', closeOutside);
        document.addEventListener('keydown', onKeyDown);
        return () => {
          document.removeEventListener('mousedown', closeOutside);
          document.removeEventListener('keydown', onKeyDown);
        };
      }, [open]);

      const presets = data?.presets || [];
      if (presets.length === 0) return null;

      const currentPresetId = (sessionId && data?.sessionPresets?.[sessionId]) || data?.activePresetId || presets[0].id;
      const currentPreset = presets.find((p) => p.id === currentPresetId) || presets[0];

      const choose = (presetId) => {
        setOpen(false);
        if (presetId === currentPresetId) return;
        setData((prev) => (prev ? {
          ...prev,
          sessionPresets: sessionId ? { ...prev.sessionPresets, [sessionId]: presetId } : prev.sessionPresets,
        } : prev));

        fetch('/api/style-control/session', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ sessionId, presetId }),
        }).catch(() => {});
      };

      // Inline styles for pixel-perfect match with ModelSelect
      const rootStyle = {
        position: 'relative',
        display: 'inline-flex',
        alignItems: 'center',
        flexShrink: 0,
        fontFamily: 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
      };

      const triggerStyle = {
        height: '28px',
        color: (hovered || open)
          ? 'var(--dsw-alias-label-primary, #1c1c1e)'
          : 'var(--dsw-alias-label-secondary, #61666b)',
        cursor: 'pointer',
        background: (hovered || open)
          ? 'var(--dsw-alias-interactive-bg-hover, rgba(125, 125, 125, 0.12))'
          : 'transparent',
        border: 'none',
        borderRadius: '24px',
        outline: 'none',
        alignItems: 'center',
        gap: '4px',
        padding: '0 6px 0 8px',
        fontSize: '13px',
        fontWeight: 500,
        lineHeight: '20px',
        display: 'inline-flex',
        transition: 'background-color .12s, color .12s',
        boxSizing: 'border-box',
        userSelect: 'none',
      };

      const menuStyle = {
        zIndex: 9999,
        border: '1px solid var(--dsw-alias-border-l1, rgba(125, 125, 125, 0.22))',
        background: 'var(--dsw-specific-menu, var(--dsw-alias-bg-layer-1, #1e1e20))',
        width: 'max-content',
        minWidth: '220px',
        maxWidth: '320px',
        maxHeight: '340px',
        boxShadow: 'var(--dsw-shadow-lv3, 0 10px 30px rgba(0, 0, 0, 0.28))',
        color: 'var(--dsw-alias-label-primary, #1c1c1e)',
        borderRadius: '12px',
        flexDirection: 'column',
        padding: '5px',
        display: 'flex',
        position: 'absolute',
        bottom: 'calc(100% + 8px)',
        right: 0,
        overflowY: 'auto',
        boxSizing: 'border-box',
      };

      return React.createElement(
        'div',
        { ref: rootRef, style: rootStyle },
        React.createElement(
          'button',
          {
            type: 'button',
            style: triggerStyle,
            'aria-haspopup': 'menu',
            'aria-expanded': open,
            title: `Formatting Style: ${currentPreset.name}`,
            onMouseEnter: () => setHovered(true),
            onMouseLeave: () => setHovered(false),
            onClick: () => setOpen((o) => !o),
          },
          // Style label and separator
          React.createElement('span', {
            style: {
              color: 'var(--dsw-alias-label-caption, #8e8e93)',
              fontSize: '12px',
              fontWeight: 450,
            },
          }, 'Style'),
          React.createElement('span', {
            style: {
              color: 'var(--dsw-alias-label-caption, #8e8e93)',
              margin: '0 1px',
              fontSize: '11px',
            },
          }, '·'),
          // Preset name
          React.createElement('span', {
            style: {
              color: 'var(--dsw-alias-label-primary, #1c1c1e)',
              fontSize: '13px',
              fontWeight: 500,
              maxWidth: '120px',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
            },
          }, currentPreset.name),
          // Subtle chevron icon matching ModelSelect
          React.createElement(
            'svg',
            {
              width: 12,
              height: 12,
              viewBox: '0 0 16 16',
              fill: 'currentColor',
              style: {
                color: 'var(--dsw-alias-label-caption, #8e8e93)',
                flexShrink: 0,
                transition: 'transform .12s ease',
                transform: open ? 'rotate(180deg)' : 'none',
                marginLeft: '1px',
              },
            },
            React.createElement('path', {
              fillRule: 'evenodd',
              d: 'M1.646 4.646a.5.5 0 0 1 .708 0L8 10.293l5.646-5.647a.5.5 0 0 1 .708.708l-6 6a.5.5 0 0 1-.708 0l-6-6a.5.5 0 0 1 0-.708z',
            })
          )
        ),
        open && React.createElement(
          'div',
          { style: menuStyle, role: 'menu' },
          presets.map((p) => {
            const selected = p.id === currentPresetId;
            const itemHovered = hoveredPresetId === p.id;
            const itemStyle = {
              width: '100%',
              padding: '8px 10px',
              border: 'none',
              borderRadius: '8px',
              background: itemHovered
                ? 'var(--dsw-alias-interactive-bg-hover, rgba(125, 125, 125, 0.14))'
                : selected
                ? 'var(--dsw-alias-interactive-bg-active, rgba(125, 125, 125, 0.08))'
                : 'transparent',
              color: 'var(--dsw-alias-label-primary, #1c1c1e)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: '12px',
              cursor: 'pointer',
              textAlign: 'left',
              outline: 'none',
              transition: 'background-color .1s',
              boxSizing: 'border-box',
              fontFamily: 'inherit',
            };

            return React.createElement(
              'button',
              {
                key: p.id,
                type: 'button',
                role: 'menuitemradio',
                'aria-checked': selected,
                style: itemStyle,
                onMouseEnter: () => setHoveredPresetId(p.id),
                onMouseLeave: () => setHoveredPresetId(null),
                onClick: () => choose(p.id),
              },
              React.createElement(
                'div',
                { style: { display: 'flex', flexDirection: 'column', gap: '2px', overflow: 'hidden' } },
                React.createElement('span', {
                  style: {
                    fontSize: '13px',
                    fontWeight: 550,
                    color: 'var(--dsw-alias-label-primary, #1c1c1e)',
                    whiteSpace: 'nowrap',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                  },
                }, p.name),
                p.content
                  ? React.createElement(
                      'span',
                      {
                        style: {
                          fontSize: '11px',
                          color: 'var(--dsw-alias-label-secondary, #6d6d72)',
                          whiteSpace: 'nowrap',
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          lineHeight: '14px',
                        },
                      },
                      p.content.slice(0, 48) + (p.content.length > 48 ? '…' : '')
                    )
                  : null
              ),
              selected
                ? React.createElement(
                    'svg',
                    {
                      width: 14,
                      height: 14,
                      viewBox: '0 0 16 16',
                      fill: '#007aff',
                      style: { flexShrink: 0 },
                    },
                    React.createElement('path', {
                      d: 'M13.485 3.515a1 1 0 0 1 0 1.414l-6.364 6.364a1 1 0 0 1-1.414 0L2.515 8.1a1 1 0 1 1 1.414-1.414l2.479 2.478 5.663-5.649a1 1 0 0 1 1.414 0z',
                    })
                  )
                : null
            );
          })
        )
      );
    }

    return {
      name: '@devvm/dsh-style-control',
      inject: ['slots'],
      apply(ctx) {
        const slots = ctx.get('slots');
        if (!slots) return;

        // Settings UI section tab
        slots.inject('settings.section', () => slots.register(
          {
            name: 'settings.section',
            id: 'style-control-section',
            order: 22,
            label: () => 'Style Control',
          },
          () => React.createElement(StyleSettingsPage, null)
        ));

        // Chat UI input bar dropdown (to the left of model selector)
        slots.inject('conversation.input.right', () => slots.register(
          {
            name: 'conversation.input.right',
            id: 'style-control-chat',
            order: -10,
            label: 'Style Control',
          },
          (props) => React.createElement(StyleChatDropdown, props)
        ));
      },
    };
  },
});
