import { apiFetch } from './api.js';

const request = async (path, method, body) => {
    const response = await apiFetch(path, {
        method,
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const data = await response.json().catch(() => null);
    if (!response.ok) {
        throw new Error(data?.error || `${method} ${path} failed (${response.status})`);
    }
    if (method !== 'DELETE' && data === null) {
        throw new Error(`${method} ${path} returned an invalid response`);
    }
    return data;
};

export const createResourceApi = (path) => ({
    list: () => request(path, 'GET'),
    create: (item) => saveItem(path, 'POST', item),
    update: (item) => saveItem(path, 'PUT', item),
    remove: (id) => request(`${path}/${encodeURIComponent(id)}`, 'DELETE'),
});

export const createSiteBindingApi = () => {
    const collection = (siteId) => `/api/sites/${encodeURIComponent(siteId)}/bindings`;
    const relationship = (siteId, bindingId) => `${collection(siteId)}/${encodeURIComponent(bindingId)}`;
    return {
        list: async (siteId) => {
            const ids = await request(collection(siteId), 'GET');
            if (!Array.isArray(ids) || ids.some((id) => typeof id !== 'string')) throw new Error('Invalid site bindings response');
            return ids.map((bindingId) => ({ site_id: siteId, binding_id: bindingId }));
        },
        attach: (relation) => request(relationship(relation.site_id, relation.binding_id), 'PUT'),
        detach: (relation) => request(relationship(relation.site_id, relation.binding_id), 'DELETE'),
    };
};

export const globalConfiguration = ({ bindings, sites, binding_sites, php_cgi_handlers, ...settings }) => settings;

export const loadEditorResources = async (bindingApi, siteApi, managedSystemApi) => {
    const [settings, bindings, sites, managedSystems] = await Promise.all([request('/config', 'GET'), bindingApi.list(), siteApi.list(), managedSystemApi.list()]);
    if (!settings || typeof settings !== 'object' || Array.isArray(settings)) throw new Error('Invalid configuration response');
    if (!Array.isArray(bindings)) throw new Error('Invalid bindings response');
    if (!Array.isArray(sites)) throw new Error('Invalid sites response');
    if (!Array.isArray(managedSystems)) throw new Error('Invalid managed systems response');
    return { config: { ...globalConfiguration(settings), sites, binding_sites: [], php_cgi_handlers: managedSystems }, bindings };
};

const saveItem = async (path, method, item) => {
    const saved = await request(path, method, item);
    if (!saved || saved.id !== item.id) throw new Error(`${method} ${path} returned an invalid resource`);
    return saved;
};

// Report each successful operation so a partial save can be retried safely.
export const saveResourceChanges = async (api, original, draft, onSaved) => {
    const originalById = new Map(original.map((item) => [item.id, item]));
    const draftById = new Map(draft.map((item) => [item.id, item]));

    for (const item of original) {
        if (!draftById.has(item.id)) {
            await api.remove(item.id);
            onSaved('remove', item);
        }
    }
    for (const item of draft) {
        const previous = originalById.get(item.id);
        if (!previous) {
            onSaved('create', await api.create(item));
        } else if (JSON.stringify(previous) !== JSON.stringify(item)) {
            onSaved('update', await api.update(item));
        }
    }
};

export const saveEditorResources = async ({ bindingApi, siteApi, managedSystemApi, relationshipApi, originalBindings, bindings, originalManagedSystems, managedSystems, originalSites, sites, originalRelationships, relationships, onBindingSaved, onManagedSystemSaved, onSiteSaved, onRelationshipSaved }) => {
    const key = (relation) => JSON.stringify([relation.site_id, relation.binding_id]);
    const originalKeys = new Set(originalRelationships.map(key));
    const draftKeys = new Set(relationships.map(key));

    for (const relation of originalRelationships.filter((item) => !draftKeys.has(key(item)))) {
        await relationshipApi.detach(relation);
        onRelationshipSaved('remove', relation);
    }
    await saveResourceChanges(bindingApi, originalBindings, bindings, onBindingSaved);
    await saveResourceChanges(managedSystemApi, originalManagedSystems, managedSystems, onManagedSystemSaved);
    await saveResourceChanges(siteApi, originalSites, sites, onSiteSaved);
    for (const relation of relationships.filter((item) => !originalKeys.has(key(item)))) {
        await relationshipApi.attach(relation);
        onRelationshipSaved('create', relation);
    }
};
