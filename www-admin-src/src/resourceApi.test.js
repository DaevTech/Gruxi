import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createResourceApi, createSiteBindingApi, globalConfiguration, loadEditorResources, saveEditorResources } from './resourceApi.js';

const resource = (id, value = id) => ({ id, value });
const relation = (site_id, binding_id) => ({ site_id, binding_id });

test('editor loads resources without prefetching site relationships', async () => {
    const originalFetch = globalThis.fetch;
    const calls = [];
    globalThis.fetch = async (path, init) => {
        calls.push([path, init.method]);
        const responses = {
            '/config': { core: { enabled: true }, bindings: [resource('stale')], sites: [resource('stale')], binding_sites: [] },
            '/api/bindings': [resource('binding')],
            '/api/sites': [resource('site')],
        };
        assert.ok(Object.hasOwn(responses, path), `Unexpected request ${path}`);
        return { ok: true, json: async () => responses[path] };
    };
    try {
        const result = await loadEditorResources(createResourceApi('/api/bindings'), createResourceApi('/api/sites'));
        assert.deepEqual(result.bindings, [resource('binding')]);
        assert.deepEqual(result.config.sites, [resource('site')]);
        assert.deepEqual(result.config.binding_sites, []);
        assert.deepEqual(globalConfiguration(result.config), { core: { enabled: true } });
        assert.deepEqual(calls, [
            ['/config', 'GET'], ['/api/bindings', 'GET'], ['/api/sites', 'GET'],
        ]);
    } finally {
        globalThis.fetch = originalFetch;
    }
});

const fixture = () => {
    const calls = [];
    const state = {
        originalBindings: [resource('old-binding')],
        bindings: [resource('new-binding')],
        originalSites: [resource('old-site')],
        sites: [resource('new-site')],
        originalRelationships: [relation('old-site', 'old-binding')],
        relationships: [relation('new-site', 'new-binding')],
    };
    const api = (type) => ({
        remove: async (id) => { calls.push(`${type}:remove:${id}`); },
        create: async (item) => { calls.push(`${type}:create:${item.id}`); return item; },
        update: async (item) => { calls.push(`${type}:update:${item.id}`); return item; },
    });
    const update = (field) => (operation, item) => {
        state[field] = state[field].filter((old) => old.id !== item.id);
        if (operation !== 'remove') state[field].push(item);
    };
    const args = () => ({
        bindingApi: api('binding'), siteApi: api('site'),
        relationshipApi: {
            detach: async (item) => { calls.push(`detach:${item.site_id}:${item.binding_id}`); },
            attach: async (item) => { calls.push(`attach:${item.site_id}:${item.binding_id}`); },
        },
        ...state,
        onBindingSaved: update('originalBindings'),
        onSiteSaved: update('originalSites'),
        onRelationshipSaved: (operation, item) => {
            state.originalRelationships = state.originalRelationships.filter((old) => old.site_id !== item.site_id || old.binding_id !== item.binding_id);
            if (operation !== 'remove') state.originalRelationships.push(item);
        },
    });
    return { calls, state, args };
};

test('detaches before deleting resources and attaches after creating them', async () => {
    const f = fixture();
    await saveEditorResources(f.args());
    assert.deepEqual(f.calls, [
        'detach:old-site:old-binding', 'binding:remove:old-binding', 'binding:create:new-binding',
        'site:remove:old-site', 'site:create:new-site', 'attach:new-site:new-binding',
    ]);
    assert.deepEqual(f.state.originalRelationships, f.state.relationships);
    f.calls.length = 0;
    await saveEditorResources(f.args());
    assert.deepEqual(f.calls, []);
});

test('a failed association can be retried without repeating saved operations', async () => {
    const f = fixture();
    const first = f.args();
    first.relationshipApi.attach = async () => { throw new Error('attach failed'); };
    await assert.rejects(saveEditorResources(first), /attach failed/);
    assert.deepEqual(f.state.originalBindings, f.state.bindings);
    assert.deepEqual(f.state.originalSites, f.state.sites);
    assert.deepEqual(f.state.originalRelationships, []);
    f.calls.length = 0;
    await saveEditorResources(f.args());
    assert.deepEqual(f.calls, ['attach:new-site:new-binding']);
});

test('site binding API uses encoded paths and propagates errors', async () => {
    const originalFetch = globalThis.fetch;
    const originalDocument = globalThis.document;
    const calls = [];
    globalThis.document = { cookie: '__Host-gruxi_admin_csrf=token' };
    globalThis.fetch = async (path, init) => {
        calls.push([path, init.method, init.headers.get('X-CSRF-Token')]);
        return { ok: true, json: async () => init.method === 'GET' ? ['binding/1'] : {} };
    };
    try {
        const api = createSiteBindingApi();
        assert.deepEqual(await api.list('site/1'), [relation('site/1', 'binding/1')]);
        await api.attach(relation('site/1', 'binding/1'));
        await api.detach(relation('site/1', 'binding/1'));
        assert.deepEqual(calls, [
            ['/api/sites/site%2F1/bindings', 'GET', null],
            ['/api/sites/site%2F1/bindings/binding%2F1', 'PUT', 'token'],
            ['/api/sites/site%2F1/bindings/binding%2F1', 'DELETE', 'token'],
        ]);
        globalThis.fetch = async () => ({ ok: false, status: 404, json: async () => ({ error: 'Binding not found' }) });
        await assert.rejects(api.attach(relation('site', 'missing')), /Binding not found/);
    } finally {
        globalThis.fetch = originalFetch;
        globalThis.document = originalDocument;
    }
});
