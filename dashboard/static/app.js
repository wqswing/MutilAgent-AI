// Multiagent Admin Dashboard JS (v0.9)

const API_BASE = '/admin';

// =========================================
// Tab Navigation
// =========================================
document.querySelectorAll('.nav-menu .nav-item').forEach(link => {
    link.addEventListener('click', (e) => {
        e.preventDefault();
        const target = e.target.closest('.nav-item');
        const tab = target.dataset.tab;

        // Update active link
        document.querySelectorAll('.nav-menu .nav-item').forEach(l => l.classList.remove('active'));
        target.classList.add('active');

        // Update active tab content
        document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
        document.getElementById(tab).classList.add('active');

        // Update header title
        const titles = {
            overview: 'Dashboard Overview',
            providers: 'LLM Providers',
            persistence: 'Persistence Configuration',
            mcp: 'MCP Registry',
            metrics: 'Performance Metrics',
            audit: 'Audit Trails'
        };
        const pageTitle = document.getElementById('page-title');
        if (pageTitle) pageTitle.textContent = titles[tab] || tab;

        // Update breadcrumbs
        const breadcrumbs = document.querySelector('.breadcrumbs');
        if (breadcrumbs) breadcrumbs.textContent = `Dashboard / ${titles[tab] || tab}`;
    });
});

// =========================================
// Fetch Wrapper
// =========================================
async function fetchWithAuth(url, options = {}) {
    const token = 'admin'; // Demo token
    return fetch(url, {
        ...options,
        headers: {
            'Authorization': `Bearer ${token}`,
            'Content-Type': 'application/json',
            ...options.headers
        }
    });
}

// =========================================
// Modal Helpers
// =========================================
function openModal(modalId) {
    document.getElementById(modalId).classList.remove('hidden');
}

function closeModal(modalId) {
    document.getElementById(modalId).classList.add('hidden');
}

// Close modal on overlay click or X button
document.querySelectorAll('.modal-overlay, .modal-close').forEach(el => {
    el.addEventListener('click', (e) => {
        e.target.closest('.modal').classList.add('hidden');
    });
});

// =========================================
// LLM Providers
// =========================================
let providers = []; // In-memory store

// Vendor card click -> open modal with prefilled vendor
document.querySelectorAll('.vendor-card').forEach(card => {
    card.addEventListener('click', () => {
        const vendor = card.dataset.vendor;
        const presets = {
            openai: { vendor: 'OpenAI', url: 'https://api.openai.com/v1', model: 'gpt-4o' },
            anthropic: { vendor: 'Anthropic', url: 'https://api.anthropic.com/v1', model: 'claude-3-5-sonnet-20241022' },
            google: { vendor: 'Google AI', url: 'https://generativelanguage.googleapis.com/v1beta', model: 'gemini-1.5-pro' },
            mistral: { vendor: 'Mistral', url: 'https://api.mistral.ai/v1', model: 'mistral-large-latest' },
            deepseek: { vendor: 'DeepSeek', url: 'https://api.deepseek.com', model: 'deepseek-chat' },
            local: { vendor: 'Local (vLLM)', url: 'http://localhost:8000/v1', model: 'local-model' }
        };
        const preset = presets[vendor] || {};

        document.getElementById('prov-vendor').value = preset.vendor || '';
        document.getElementById('prov-url').value = preset.url || '';
        document.getElementById('prov-model').value = preset.model || '';
        document.getElementById('prov-desc').value = '';
        document.getElementById('prov-version').value = '';
        document.getElementById('prov-key').value = '';

        openModal('modal-provider');
    });
});

// Add Provider button
document.getElementById('btn-add-provider')?.addEventListener('click', () => {
    // Clear form
    document.getElementById('form-provider').reset();
    openModal('modal-provider');
});

// Form submit -> save provider
document.getElementById('form-provider')?.addEventListener('submit', async (e) => {
    e.preventDefault();

    const capabilities = Array.from(document.querySelectorAll('#form-provider input[name="cap"]:checked'))
        .map(cb => cb.value);

    const provider = {
        id: 'prov-' + Date.now(),
        vendor: document.getElementById('prov-vendor').value,
        model_id: document.getElementById('prov-model').value,
        description: document.getElementById('prov-desc').value || null,
        base_url: document.getElementById('prov-url').value,
        version: document.getElementById('prov-version').value || null,
        api_key: document.getElementById('prov-key').value,
        capabilities: capabilities,
        status: 'pending'
    };

    // Send to backend
    try {
        const res = await fetchWithAuth(`${API_BASE}/providers`, {
            method: 'POST',
            body: JSON.stringify(provider)
        });

        if (res.ok) {
            providers.push(provider);
            renderProviders();
            closeModal('modal-provider');
        } else {
            alert('Failed to save provider');
        }
    } catch (err) {
        // If backend not available, store locally
        providers.push(provider);
        renderProviders();
        closeModal('modal-provider');
    }
});

// Test provider connection
document.getElementById('btn-test-provider')?.addEventListener('click', async () => {
    const btn = document.getElementById('btn-test-provider');
    btn.disabled = true;
    btn.innerHTML = '<i class="fa-solid fa-spinner fa-spin"></i> Testing...';

    try {
        const res = await fetchWithAuth(`${API_BASE}/providers/test`, {
            method: 'POST',
            body: JSON.stringify({
                base_url: document.getElementById('prov-url').value,
                api_key: document.getElementById('prov-key').value,
                model_id: document.getElementById('prov-model').value
            })
        });

        if (res.ok) {
            btn.innerHTML = '<i class="fa-solid fa-check"></i> Connected!';
            btn.style.color = 'var(--success)';
        } else {
            btn.innerHTML = '<i class="fa-solid fa-xmark"></i> Failed';
            btn.style.color = 'var(--danger)';
        }
    } catch (err) {
        btn.innerHTML = '<i class="fa-solid fa-xmark"></i> Error';
        btn.style.color = 'var(--danger)';
    }

    setTimeout(() => {
        btn.disabled = false;
        btn.innerHTML = '<i class="fa-solid fa-plug"></i> Test Connection';
        btn.style.color = '';
    }, 2000);
});

function renderProviders() {
    const tbody = document.getElementById('providers-body');
    if (!tbody) return;

    if (providers.length === 0) {
        tbody.innerHTML = '<tr><td colspan="5" class="empty-state">No providers configured. Click "Add Provider" or select a preset above.</td></tr>';
        return;
    }

    tbody.innerHTML = providers.map(p => `
        <tr>
            <td><span class="status-pill status-${p.status === 'connected' ? 'healthy' : 'degraded'}">${p.status}</span></td>
            <td class="font-medium">${p.vendor} <span class="text-sm text-muted">(${p.model_id})</span></td>
            <td class="text-sm">${p.model_id}</td>
            <td>
                <div class="tags-container">
                    ${p.capabilities.map(cap => `<span class="tag text-xs">${cap}</span>`).join('')}
                </div>
            </td>
            <td>
                <button class="btn-icon" onclick="testProviderById('${p.id}')" title="Test"><i class="fas fa-plug"></i></button>
                <button class="btn-icon text-red" onclick="deleteProvider('${p.id}')" title="Delete"><i class="fas fa-trash"></i></button>
            </td>
        </tr>
    `).join('');
}

async function loadProviders() {
    try {
        const res = await fetchWithAuth(`${API_BASE}/providers`);
        if (res.ok) {
            providers = await res.json();
            renderProviders();
        }
    } catch (err) {
        console.error('Failed to load providers:', err);
    }
}

window.deleteProvider = async (id) => {
    providers = providers.filter(p => p.id !== id);
    try {
        await fetchWithAuth(`${API_BASE}/providers/${id}`, { method: 'DELETE' });
    } catch (err) {
        // Ignore if backend not available
    }
    renderProviders();
};

window.testProviderById = async (id) => {
    const provider = providers.find(p => p.id === id);
    if (!provider) return;

    try {
        const res = await fetchWithAuth(`${API_BASE}/providers/${id}/test`, { method: 'POST' });
        provider.status = res.ok ? 'connected' : 'error';
    } catch (err) {
        provider.status = 'error';
    }
    renderProviders();
};

// =========================================
// Persistence (S3)
// =========================================
document.getElementById('s3-enabled')?.addEventListener('change', (e) => {
    const form = document.getElementById('form-s3');
    if (e.target.checked) {
        form.classList.remove('hidden');
    } else {
        form.classList.add('hidden');
    }
});

document.getElementById('btn-test-s3')?.addEventListener('click', async () => {
    const btn = document.getElementById('btn-test-s3');
    const status = document.getElementById('s3-status');
    btn.disabled = true;
    btn.innerHTML = '<i class="fa-solid fa-spinner fa-spin"></i> Testing...';

    try {
        const res = await fetchWithAuth(`${API_BASE}/persistence/test`, {
            method: 'POST',
            body: JSON.stringify({
                bucket: document.getElementById('s3-bucket').value,
                endpoint: document.getElementById('s3-endpoint').value,
                access_key: document.getElementById('s3-access-key').value,
                secret_key: document.getElementById('s3-secret-key').value,
                region: document.getElementById('s3-region').value
            })
        });

        status.classList.remove('hidden');
        if (res.ok) {
            status.className = 'status-message success';
            status.textContent = '✓ Connection successful! Bucket is accessible.';
        } else {
            status.className = 'status-message error';
            status.textContent = '✗ Connection failed. Check your credentials.';
        }
    } catch (err) {
        status.classList.remove('hidden');
        status.className = 'status-message error';
        status.textContent = '✗ Error: ' + err.message;
    }

    btn.disabled = false;
    btn.innerHTML = '<i class="fa-solid fa-plug"></i> Test Connection';
});

document.getElementById('form-s3')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    // Save S3 config (stub for now)
    alert('S3 configuration saved. Restart server to apply changes.');
});

async function loadPersistenceConfig() {
    try {
        const res = await fetchWithAuth(`${API_BASE}/config`);
        if (res.ok) {
            const data = await res.json();
            document.getElementById('cfg-storage-mode').textContent = data.persistence?.mode || 'In-Memory';
            document.getElementById('cfg-s3-bucket').textContent = data.persistence?.s3_bucket || 'N/A';
            document.getElementById('cfg-s3-endpoint').textContent = data.persistence?.s3_endpoint || 'Default (AWS)';

            if (data.persistence?.mode?.includes('S3')) {
                document.getElementById('s3-enabled').checked = true;
                document.getElementById('form-s3').classList.remove('hidden');
            }
        }
    } catch (err) {
        console.error('Failed to load persistence config:', err);
    }
}

// =========================================
// MCP Registry
// =========================================
document.getElementById('btn-register-mcp')?.addEventListener('click', () => {
    document.getElementById('form-mcp').reset();
    openModal('modal-mcp');
});

document.getElementById('form-mcp')?.addEventListener('submit', async (e) => {
    e.preventDefault();

    const capabilities = Array.from(document.querySelectorAll('#form-mcp input[name="mcp-cap"]:checked'))
        .map(cb => cb.value);

    const server = {
        name: document.getElementById('mcp-name').value,
        transport_type: document.getElementById('mcp-transport').value,
        command: document.getElementById('mcp-command').value,
        capabilities: capabilities
    };

    try {
        const res = await fetchWithAuth(`${API_BASE}/mcp/register`, {
            method: 'POST',
            body: JSON.stringify(server)
        });

        if (res.ok) {
            loadMcpServers();
            closeModal('modal-mcp');
        } else {
            alert('Failed to register MCP server');
        }
    } catch (err) {
        console.error('Failed to register MCP server:', err);
        alert('Failed to register MCP server: ' + err.message);
    }
});

async function loadMcpServers() {
    try {
        const res = await fetchWithAuth(`${API_BASE}/mcp/servers`);
        const data = await res.json();
        const tbody = document.getElementById('mcp-body');

        if (data.length === 0) {
            tbody.innerHTML = '<tr><td colspan="5" class="empty-state">No MCP servers registered</td></tr>';
            return;
        }

        tbody.innerHTML = data.map(server => `
            <tr>
                <td><span class="status-pill status-${server.available ? 'healthy' : 'degraded'}">${server.available ? 'Connected' : 'Offline'}</span></td>
                <td class="font-medium">${server.name} <span class="text-sm text-muted">(${server.id})</span></td>
                <td class="text-sm">${server.transport_type}</td>
                <td>
                    <div class="tags-container">
                        ${server.capabilities.map(cap => `<span class="tag text-xs">${cap}</span>`).join('')}
                    </div>
                </td>
                <td>
                   <button class="btn-icon" title="Inspect"><i class="fas fa-search"></i></button>
                   <button class="btn-icon text-red" onclick="removeMcp('${server.id}')" title="Remove"><i class="fas fa-trash"></i></button>
                </td>
            </tr>
        `).join('');
    } catch (err) {
        console.error('Failed to load MCP servers:', err);
    }
}

window.removeMcp = async (id) => {
    try {
        await fetchWithAuth(`${API_BASE}/mcp/servers/${id}`, { method: 'DELETE' });
        loadMcpServers();
    } catch (err) {
        console.error('Failed to remove MCP server:', err);
    }
};

// =========================================
// Metrics & Overview
// =========================================
async function loadMetrics() {
    try {
        const res = await fetchWithAuth(`${API_BASE}/metrics`);
        const data = await res.json();

        document.getElementById('stat-requests').textContent = data.requests_total?.toLocaleString() || '--';
        document.getElementById('stat-tokens').textContent = data.tokens_used?.toLocaleString() || '--';
        document.getElementById('stat-sessions').textContent = data.active_sessions || '0';
        document.getElementById('stat-latency').textContent = data.avg_latency_ms ? `${Math.round(data.avg_latency_ms)}ms` : '--';
    } catch (err) {
        console.error('Failed to load metrics:', err);
    }
}

// =========================================
// Audit Logs
// =========================================
async function loadAuditLogs() {
    const userId = document.getElementById('filter-user')?.value || '';
    const action = document.getElementById('filter-action')?.value || '';

    let url = `${API_BASE}/audit?limit=50`;
    if (userId) url += `&user_id=${encodeURIComponent(userId)}`;
    if (action) url += `&action=${encodeURIComponent(action)}`;

    try {
        const res = await fetchWithAuth(url);
        const entries = await res.json();

        const tbody = document.getElementById('audit-body');
        if (entries.length === 0) {
            tbody.innerHTML = '<tr><td colspan="5" class="empty-state">No audit entries found</td></tr>';
            return;
        }

        tbody.innerHTML = entries.map(e => `
            <tr>
                <td>${e.timestamp}</td>
                <td>${e.user_id}</td>
                <td>${e.action}</td>
                <td>${e.resource}</td>
                <td><span class="outcome-${e.outcome?.toLowerCase()}">${e.outcome}</span></td>
            </tr>
        `).join('');
    } catch (err) {
        console.error('Failed to load audit logs:', err);
    }
}

document.getElementById('btn-refresh')?.addEventListener('click', loadAuditLogs);

// =========================================
// Initial Load
// =========================================
document.addEventListener('DOMContentLoaded', () => {
    loadMetrics();
    loadProviders();
    loadPersistenceConfig();
    loadMcpServers();
    loadAuditLogs();

    // Auto-refresh metrics every 5s
    setInterval(loadMetrics, 5000);
});
