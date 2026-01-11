<script setup>
import { ref, reactive, computed, onMounted } from 'vue';

// Define props
const props = defineProps({
    user: {
        type: Object,
        required: true,
    },
    inline: {
        type: Boolean,
        default: false,
    },
});

// Define emits
const emit = defineEmits(['close']);

// Available rewrite function options
const rewriteFunctionOptions = ['OnlyWebRootIndexForSubdirs'];

// State
const isLoading = ref(false);
const isSaving = ref(false);
const error = ref('');
const saveErrorMessage = ref('');
const saveErrors = ref([]);
const successMessage = ref('');
const originalConfig = ref(null);
const config = ref(null);

// Track which sections are expanded (all collapsed by default)
const expandedSections = reactive({
    bindings: false,
    sites: false,
    managedExternalSystems: false,
    core: false,
});

// Track which individual items are expanded
const expandedItems = reactive({
    bindings: {},
    sites: {},
    siteProcessors: {},
    siteSubsections: {},
    phpCgiHandlers: {},
    coreSubsections: {
        fileCache: false,
        gzip: false,
    },
});

// Check if config has unsaved changes
const hasUnsavedChanges = computed(() => {
    if (!originalConfig.value || !config.value) return false;
    return JSON.stringify(originalConfig.value) !== JSON.stringify(config.value);
});

// Load configuration
const loadConfiguration = async () => {
    isLoading.value = true;
    error.value = '';

    try {
        const response = await fetch('/config', {
            method: 'GET',
            headers: {
                Authorization: `Bearer ${props.user.sessionToken}`,
                'Content-Type': 'application/json',
            },
        });

        if (response.ok) {
            const data = await response.json();
            originalConfig.value = JSON.parse(JSON.stringify(data)); // Deep copy
            config.value = data;
        } else {
            error.value = 'Failed to load configuration';
        }
    } catch (err) {
        console.error('Config loading error:', err);
        error.value = 'Network error while loading configuration';
    } finally {
        isLoading.value = false;
    }
};

// Configuration reload state
const isReloading = ref(false);
const showReloadModal = ref(false);
const reloadError = ref('');

// Reload configuration (server restart)
const showReloadConfirmation = () => {
    showReloadModal.value = true;
};

const hideReloadConfirmation = () => {
    showReloadModal.value = false;
};

const confirmReloadConfiguration = async () => {
    isReloading.value = true;
    reloadError.value = '';
    showReloadModal.value = false;

    try {
        const response = await fetch('/configuration/reload', {
            method: 'POST',
            headers: {
                Authorization: `Bearer ${props.user.sessionToken}`,
                'Content-Type': 'application/json',
            },
        });

        if (response.ok) {
            successMessage.value = 'Configuration reload initiated. The server is restarting...';
            // Optionally reload the page after a short delay
            setTimeout(() => {
                window.location.reload();
            }, 3000);
        } else {
            const errorData = await response.json().catch(() => ({ error: 'Unknown error' }));
            reloadError.value = errorData.error || 'Failed to reload configuration';
            showReloadModal.value = true; // Show modal again to display error
        }
    } catch (err) {
        console.error('Config reload error:', err);
        reloadError.value = 'Network error while reloading configuration';
        showReloadModal.value = true; // Show modal again to display error
    } finally {
        isReloading.value = false;
    }
};

// Save configuration
const saveConfiguration = async () => {
    isSaving.value = true;
    saveErrorMessage.value = '';
    saveErrors.value = [];
    successMessage.value = '';

    try {
        const response = await fetch('/config', {
            method: 'POST',
            headers: {
                Authorization: `Bearer ${props.user.sessionToken}`,
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(config.value),
        });

        const responseData = await response.json().catch(() => ({}));

        if (response.ok) {
            // Apply the sanitized configuration returned from the server
            if (responseData.configuration) {
                config.value = responseData.configuration;
                originalConfig.value = JSON.parse(JSON.stringify(responseData.configuration));
            } else {
                // Fallback: if no configuration returned, keep current as original
                originalConfig.value = JSON.parse(JSON.stringify(config.value));
            }
            successMessage.value = responseData.message || 'Configuration saved successfully!';
            saveErrorMessage.value = ''; // Clear any previous save errors
            saveErrors.value = [];
            setTimeout(() => {
                successMessage.value = '';
            }, 10000); // Show for 10 seconds since restart might be required
        } else {
            // Handle different types of errors - DON'T reset config, keep user's changes
            if (response.status === 400) {
                // Validation error
                const rawErrors = responseData?.errors;
                const normalizedErrors = Array.isArray(rawErrors) ? rawErrors : typeof rawErrors === 'string' ? [rawErrors] : [];

                saveErrors.value = normalizedErrors.map((err) => String(err).trim()).filter((err) => err.length > 0);

                // Prefer the top-level message; fall back to the first error string.
                saveErrorMessage.value = responseData?.error || saveErrors.value[0] || 'Configuration validation failed';
            } else if (response.status === 401) {
                saveErrorMessage.value = 'Authentication required. Please log in again.';
            } else {
                saveErrorMessage.value = responseData?.error || 'Failed to save configuration';
            }
            successMessage.value = ''; // Clear success message if there's an error
        }
    } catch (err) {
        console.error('Config saving error:', err);
        successMessage.value = ''; // Clear success message if there's an error
        if (err.name === 'TypeError' && err.message.includes('Failed to fetch')) {
            saveErrorMessage.value = 'Network error: Unable to connect to server';
        } else {
            saveErrorMessage.value = 'Network error while saving configuration';
        }
    } finally {
        isSaving.value = false;
    }
};

// Reset changes
const resetChanges = () => {
    if (originalConfig.value) {
        config.value = JSON.parse(JSON.stringify(originalConfig.value));
    }
};

// Toggle section expansion
const toggleSection = (section) => {
    expandedSections[section] = !expandedSections[section];
};

// Toggle individual item expansion
const toggleBinding = (bindingIndex) => {
    if (!expandedItems.bindings[bindingIndex]) {
        expandedItems.bindings[bindingIndex] = false;
    }
    expandedItems.bindings[bindingIndex] = !expandedItems.bindings[bindingIndex];
};

const toggleSite = (siteIndex) => {
    if (!expandedItems.sites[siteIndex]) {
        expandedItems.sites[siteIndex] = false;
    }
    expandedItems.sites[siteIndex] = !expandedItems.sites[siteIndex];
};

const togglePhpCgiHandler = (handlerIndex) => {
    if (!expandedItems.phpCgiHandlers[handlerIndex]) {
        expandedItems.phpCgiHandlers[handlerIndex] = false;
    }
    expandedItems.phpCgiHandlers[handlerIndex] = !expandedItems.phpCgiHandlers[handlerIndex];
};

const toggleSiteProcessor = (siteIndex, processorIndex) => {
    const key = `${siteIndex}-${processorIndex}`;
    if (!expandedItems.siteProcessors[key]) {
        expandedItems.siteProcessors[key] = false;
    }
    expandedItems.siteProcessors[key] = !expandedItems.siteProcessors[key];
};

// Helper functions to check if items are expanded
const isBindingExpanded = (bindingIndex) => {
    return expandedItems.bindings[bindingIndex] || false;
};

const isSiteExpanded = (siteIndex) => {
    return expandedItems.sites[siteIndex] || false;
};

const isPhpCgiHandlerExpanded = (handlerIndex) => {
    return expandedItems.phpCgiHandlers[handlerIndex] || false;
};

const isSiteProcessorExpanded = (siteIndex, processorIndex) => {
    const key = `${siteIndex}-${processorIndex}`;
    return expandedItems.siteProcessors[key] || false;
};

// Toggle site subsections (processors, TLS)
const toggleSiteSubsection = (siteIndex, subsection) => {
    const key = `${siteIndex}-${subsection}`;
    if (!expandedItems.siteSubsections[key]) {
        expandedItems.siteSubsections[key] = false;
    }
    expandedItems.siteSubsections[key] = !expandedItems.siteSubsections[key];
};

const isSiteSubsectionExpanded = (siteIndex, subsection) => {
    const key = `${siteIndex}-${subsection}`;
    return expandedItems.siteSubsections[key] || false;
};

// Toggle core subsections
const toggleCoreSubsection = (subsection) => {
    expandedItems.coreSubsections[subsection] = !expandedItems.coreSubsections[subsection];
};

const isCoreSubsectionExpanded = (subsection) => {
    return expandedItems.coreSubsections[subsection] || false;
};

const normalizeSiteRequestHandlersOrder = (siteIndex) => {
    if (!config.value?.sites?.[siteIndex]) return;

    const site = config.value.sites[siteIndex];
    if (!Array.isArray(site.request_handlers) || site.request_handlers.length === 0) return;
    if (!Array.isArray(config.value.request_handlers)) return;

    const handlerById = new Map(config.value.request_handlers.map((h) => [h.id, h]));

    const uniquePreserveOrder = (items) => {
        const seen = new Set();
        const out = [];
        for (const item of items) {
            if (seen.has(item)) continue;
            seen.add(item);
            out.push(item);
        }
        return out;
    };

    const ids = site.request_handlers.filter((id) => id !== null && id !== undefined && String(id).length > 0);
    const knownIds = uniquePreserveOrder(ids.filter((id) => handlerById.has(id)));
    const unknownIds = uniquePreserveOrder(ids.filter((id) => !handlerById.has(id)));

    // Ordering is now defined solely by the order of handler IDs on the site.
    site.request_handlers = [...knownIds, ...unknownIds];
};

const moveProcessorOrder = (siteIndex, requestHandlerId, direction) => {
    if (!config.value?.sites?.[siteIndex]) return;
    const site = config.value.sites[siteIndex];
    if (!Array.isArray(site.request_handlers) || site.request_handlers.length === 0) return;
    if (!Array.isArray(config.value.request_handlers)) return;

    normalizeSiteRequestHandlersOrder(siteIndex);

    const handlerIdSet = new Set(config.value.request_handlers.map((h) => h.id));
    const currentIndex = site.request_handlers.findIndex((id) => id === requestHandlerId);
    if (currentIndex === -1) return;

    const step = direction === 'up' ? -1 : 1;
    let targetIndex = currentIndex + step;
    while (targetIndex >= 0 && targetIndex < site.request_handlers.length && !handlerIdSet.has(site.request_handlers[targetIndex])) {
        targetIndex += step;
    }
    if (targetIndex < 0 || targetIndex >= site.request_handlers.length) return;

    const tmp = site.request_handlers[currentIndex];
    site.request_handlers[currentIndex] = site.request_handlers[targetIndex];
    site.request_handlers[targetIndex] = tmp;
};

// Get processors for a site by looking up request handlers
const getSiteProcessors = (siteIndex) => {
    if (!config.value || !config.value.sites || !config.value.sites[siteIndex]) {
        return [];
    }

    const site = config.value.sites[siteIndex];
    if (!site.request_handlers || !config.value.request_handlers) {
        return [];
    }

    // Order is defined by the site.request_handlers array.
    const resolvedHandlers = site.request_handlers.map((handlerId) => config.value.request_handlers.find((h) => h.id === handlerId)).filter((handler) => handler !== undefined);

    return resolvedHandlers;
};

// View-model for template binding: includes the handler plus a reference to its processor config object.
// This avoids v-model needing to write through a lookup function (which Vue won't allow).
const getSiteProcessorModels = (siteIndex) => {
    const handlers = getSiteProcessors(siteIndex);
    return handlers.map((handler) => {
        const processorType = handler.processor_type;
        const processorId = handler.processor_id;

        const staticConfig = processorType === 'static' ? config.value?.static_file_processors?.find((p) => p.id === processorId) : null;
        const phpConfig = processorType === 'php' ? config.value?.php_processors?.find((p) => p.id === processorId) : null;
        const proxyConfig = processorType === 'proxy' ? config.value?.proxy_processors?.find((p) => p.id === processorId) : null;

        return {
            handler,
            static_config: staticConfig,
            php_config: phpConfig,
            proxy_config: proxyConfig,
        };
    });
};

// Add new binding
const addBinding = () => {
    if (!config.value.bindings) {
        config.value.bindings = [];
    }
    const newId = Math.max(0, ...config.value.bindings.map((b) => b.id)) + 1;
    config.value.bindings.push({
        id: newId,
        ip: '0.0.0.0',
        port: 80,
        is_admin: false,
        is_tls: false,
    });
};

// Remove binding
const removeBinding = (index) => {
    if (config.value.bindings && config.value.bindings.length > index) {
        const bindingId = config.value.bindings[index].id;
        config.value.bindings.splice(index, 1);
        // Remove associated binding_sites relationships
        if (!config.value.binding_sites) {
            config.value.binding_sites = [];
        }
        config.value.binding_sites = config.value.binding_sites.filter((bs) => bs.binding_id !== bindingId);
    }
};

// Add new site
const addSite = () => {
    if (!config.value.sites) {
        config.value.sites = [];
    }
    const newId = Math.max(0, ...config.value.sites.map((s) => s.id)) + 1;

    config.value.sites.push({
        id: newId,
        hostnames: ['example.com'],
        is_default: false,
        is_enabled: true,
        tls_cert_path: '',
        tls_cert_content: '',
        tls_key_path: '',
        tls_key_content: '',
        rewrite_functions: [],
        request_handlers: [],
        extra_headers: [],
        access_log_enabled: false,
        access_log_file: '',
    });
};

// Remove site
const removeSite = (index) => {
    if (config.value.sites && config.value.sites.length > index) {
        const siteId = config.value.sites[index].id;
        config.value.sites.splice(index, 1);
        // Remove associated binding_sites relationships
        if (!config.value.binding_sites) {
            config.value.binding_sites = [];
        }
        config.value.binding_sites = config.value.binding_sites.filter((bs) => bs.site_id !== siteId);
    }
};

// ========== Managed External Systems (PHP-CGI) ==========

const addPhpCgiHandler = () => {
    if (!config.value.php_cgi_handlers) {
        config.value.php_cgi_handlers = [];
    }

    const newId = crypto.randomUUID();
    config.value.php_cgi_handlers.push({
        id: newId,
        name: 'PHP X.Y.Z Handler',
        request_timeout: 30,
        concurrent_threads: 0,
        executable: '',
    });
};

const removePhpCgiHandler = (index) => {
    if (!config.value.php_cgi_handlers || config.value.php_cgi_handlers.length <= index) return;

    const removedId = config.value.php_cgi_handlers[index].id;
    config.value.php_cgi_handlers.splice(index, 1);

    // Clear references from PHP processors that used this managed handler.
    if (Array.isArray(config.value.php_processors)) {
        for (const processor of config.value.php_processors) {
            if (processor.served_by_type === 'win-php-cgi' && processor.php_cgi_handler_id === removedId) {
                processor.php_cgi_handler_id = '';
            }
        }
    }
};

// Add hostname to site
const addHostname = (siteIndex) => {
    if (config.value.sites && config.value.sites[siteIndex]) {
        config.value.sites[siteIndex].hostnames.push('example.com');
    }
};

// Remove hostname from site
const removeHostname = (siteIndex, hostnameIndex) => {
    if (config.value.sites && config.value.sites[siteIndex] && config.value.sites[siteIndex].hostnames.length > hostnameIndex) {
        config.value.sites[siteIndex].hostnames.splice(hostnameIndex, 1);
    }
};

// Add enabled handler to site
const addEnabledHandler = (siteIndex) => {
    if (config.value.sites && config.value.sites[siteIndex]) {
        // Add the first available handler ID, or empty string if none available
        const availableHandlers = getAvailableRequestHandlers();
        const handlerId = availableHandlers.length > 0 ? availableHandlers[0].id : '';
        config.value.sites[siteIndex].request_handlers.push(handlerId);
    }
};

// Remove enabled handler from site
const removeEnabledHandler = (siteIndex, handlerIndex) => {
    if (config.value.sites && config.value.sites[siteIndex] && config.value.sites[siteIndex].request_handlers.length > handlerIndex) {
        config.value.sites[siteIndex].request_handlers.splice(handlerIndex, 1);
    }
};

// Extra headers helpers
const addExtraHeader = (siteIndex) => {
    if (config.value.sites && config.value.sites[siteIndex]) {
        if (!config.value.sites[siteIndex].extra_headers) {
            config.value.sites[siteIndex].extra_headers = [];
        }
        config.value.sites[siteIndex].extra_headers.push({ key: 'X-Header', value: 'value' });
    }
};

const removeExtraHeader = (siteIndex, headerIndex) => {
    if (config.value.sites && config.value.sites[siteIndex] && config.value.sites[siteIndex].extra_headers && config.value.sites[siteIndex].extra_headers.length > headerIndex) {
        config.value.sites[siteIndex].extra_headers.splice(headerIndex, 1);
    }
};

// Add rewrite function to site
const addRewriteFunction = (siteIndex) => {
    if (config.value.sites && config.value.sites[siteIndex]) {
        config.value.sites[siteIndex].rewrite_functions.push('OnlyWebRootIndexForSubdirs');
    }
};

// Remove rewrite function from site
const removeRewriteFunction = (siteIndex, functionIndex) => {
    if (config.value.sites && config.value.sites[siteIndex] && config.value.sites[siteIndex].rewrite_functions.length > functionIndex) {
        config.value.sites[siteIndex].rewrite_functions.splice(functionIndex, 1);
    }
};

// Get available bindings for site association
const getAvailableBindings = () => {
    return (
        config.value.bindings?.map((b) => ({
            id: b.id,
            label: `${b.ip}:${b.port}${b.is_admin ? ' (Admin)' : ''}${b.is_tls ? ' (TLS)' : ''}`,
        })) || []
    );
};

// Get available request handlers for site association
const getAvailableRequestHandlers = () => {
    return (
        config.value.request_handlers?.map((h) => ({
            id: h.id,
            name: h.name,
            handler_type: h.handler_type,
            label: `${h.name} (${h.handler_type})${!h.is_enabled ? ' - DISABLED' : ''}`,
        })) || []
    );
};

// Get handler name by ID
const getHandlerNameById = (handlerId) => {
    const handler = config.value.request_handlers?.find((h) => h.id === handlerId);
    return handler ? `${handler.name} (${handler.handler_type})` : `Handler ID: ${handlerId}`;
};

// Get bindings associated with a site
const getSiteBindings = (siteId) => {
    if (!config.value.binding_sites) return [];
    return config.value.binding_sites.filter((bs) => bs.site_id === siteId).map((bs) => bs.binding_id);
};

// Associate site with binding
const associateSiteWithBinding = (siteId, bindingId) => {
    if (!config.value.binding_sites) {
        config.value.binding_sites = [];
    }
    // Check if association already exists
    const existingAssociation = config.value.binding_sites.find((bs) => bs.site_id === siteId && bs.binding_id === bindingId);
    if (!existingAssociation) {
        config.value.binding_sites.push({ binding_id: bindingId, site_id: siteId });
    }
};

// Disassociate site from binding
const disassociateSiteFromBinding = (siteId, bindingId) => {
    if (!config.value.binding_sites) return;
    config.value.binding_sites = config.value.binding_sites.filter((bs) => !(bs.site_id === siteId && bs.binding_id === bindingId));
};

// Add gzip content type
const addGzipContentType = () => {
    if (config.value.core && config.value.core.gzip) {
        config.value.core.gzip.compressible_content_types.push('text/plain');
    }
};

// Remove gzip content type
const removeGzipContentType = (index) => {
    if (config.value.core && config.value.core.gzip && config.value.core.gzip.compressible_content_types.length > index) {
        config.value.core.gzip.compressible_content_types.splice(index, 1);
    }
};

// ========== Site Processor Management Functions ==========

// Add processor to site
const addProcessorToSite = (siteIndex, processorType) => {
    if (!config.value.sites || !config.value.sites[siteIndex]) return;

    const site = config.value.sites[siteIndex];

    // Initialize arrays if needed
    if (!site.request_handlers) {
        site.request_handlers = [];
    }
    if (!config.value.request_handlers) {
        config.value.request_handlers = [];
    }

    // Create processor object with UUID
    const processorId = crypto.randomUUID();
    let newProcessor;

    if (processorType === 'static') {
        if (!config.value.static_file_processors) {
            config.value.static_file_processors = [];
        }
        newProcessor = {
            id: processorId,
            web_root: './www-default',
            web_root_index_file_list: ['index.html', 'index.htm'],
        };
        config.value.static_file_processors.push(newProcessor);
    } else if (processorType === 'php') {
        if (!config.value.php_processors) {
            config.value.php_processors = [];
        }
        newProcessor = {
            id: processorId,
            served_by_type: 'php-fpm',
            php_cgi_handler_id: '',
            fastcgi_ip_and_port: '',
            request_timeout: 30,
            local_web_root: '',
            fastcgi_web_root: '',
        };
        config.value.php_processors.push(newProcessor);
    } else if (processorType === 'proxy') {
        if (!config.value.proxy_processors) {
            config.value.proxy_processors = [];
        }
        newProcessor = {
            id: processorId,
            proxy_type: 'http',
            upstream_servers: [],
            load_balancing_strategy: 'round_robin',
            timeout_seconds: 30,
            health_check_path: '/health',
            health_check_interval_seconds: 60,
            health_check_timeout_seconds: 5,
            url_rewrites: [],
            preserve_host_header: false,
            forced_host_header: '',
            verify_tls_certificates: true,
        };
        config.value.proxy_processors.push(newProcessor);
    }

    // Create RequestHandler that references the processor
    const requestHandlerId = crypto.randomUUID();
    const newRequestHandler = {
        id: requestHandlerId,
        is_enabled: true,
        name: `${processorType.charAt(0).toUpperCase() + processorType.slice(1)} Processor`,
        processor_type: processorType,
        processor_id: processorId,
        url_match: ['*'],
    };

    config.value.request_handlers.push(newRequestHandler);
    site.request_handlers.push(requestHandlerId);

    normalizeSiteRequestHandlersOrder(siteIndex);
};

// Remove processor from site
const removeProcessorFromSite = (siteIndex, processorIndex) => {
    const processors = getSiteProcessors(siteIndex);
    if (!processors || processorIndex >= processors.length) return;

    const requestHandler = processors[processorIndex];
    const site = config.value.sites[siteIndex];

    // Remove the request handler ID from site
    const handlerIdIndex = site.request_handlers.indexOf(requestHandler.id);
    if (handlerIdIndex !== -1) {
        site.request_handlers.splice(handlerIdIndex, 1);
    }

    // Remove the actual processor from the appropriate array
    if (requestHandler.processor_type === 'static' && config.value.static_file_processors) {
        const idx = config.value.static_file_processors.findIndex((p) => p.id === requestHandler.processor_id);
        if (idx !== -1) config.value.static_file_processors.splice(idx, 1);
    } else if (requestHandler.processor_type === 'php' && config.value.php_processors) {
        const idx = config.value.php_processors.findIndex((p) => p.id === requestHandler.processor_id);
        if (idx !== -1) config.value.php_processors.splice(idx, 1);
    } else if (requestHandler.processor_type === 'proxy' && config.value.proxy_processors) {
        const idx = config.value.proxy_processors.findIndex((p) => p.id === requestHandler.processor_id);
        if (idx !== -1) config.value.proxy_processors.splice(idx, 1);
    }

    // Remove the request handler from top level
    const rhIndex = config.value.request_handlers.findIndex((h) => h.id === requestHandler.id);
    if (rhIndex !== -1) {
        config.value.request_handlers.splice(rhIndex, 1);
    }
};

// Add URL match pattern to processor
const addUrlMatchToProcessor = (siteIndex, processorIndex, value = '*') => {
    const processors = getSiteProcessors(siteIndex);
    if (processors && processors[processorIndex] && value.trim()) {
        processors[processorIndex].url_match.push(value.trim());
    }
};

// Remove URL match pattern from processor
const removeUrlMatchFromProcessor = (siteIndex, processorIndex, matchIndex) => {
    const processors = getSiteProcessors(siteIndex);
    if (processors && processors[processorIndex]) {
        processors[processorIndex].url_match.splice(matchIndex, 1);
    }
};

// Add index file to static processor
const addIndexFileToProcessor = (siteIndex, processorIndex) => {
    const processors = getSiteProcessors(siteIndex);
    if (!processors || !processors[processorIndex]) return;

    const requestHandler = processors[processorIndex];
    if (requestHandler.processor_type === 'static' && config.value.static_file_processors) {
        const staticProcessor = config.value.static_file_processors.find((p) => p.id === requestHandler.processor_id);
        if (staticProcessor && staticProcessor.web_root_index_file_list) {
            staticProcessor.web_root_index_file_list.push('index.html');
        }
    }
};

// Remove index file from static processor
const removeIndexFileFromProcessor = (siteIndex, processorIndex, fileIndex) => {
    const processors = getSiteProcessors(siteIndex);
    if (!processors || !processors[processorIndex]) return;

    const requestHandler = processors[processorIndex];
    if (requestHandler.processor_type === 'static' && config.value.static_file_processors) {
        const staticProcessor = config.value.static_file_processors.find((p) => p.id === requestHandler.processor_id);
        if (staticProcessor && staticProcessor.web_root_index_file_list) {
            staticProcessor.web_root_index_file_list.splice(fileIndex, 1);
        }
    }
};

// MB to bytes conversion for file cache
const bytesToMb = (bytes) => {
    return Math.round((bytes / (1024 * 1024)) * 100) / 100; // Round to 2 decimal places
};

const mbToBytes = (mb) => {
    return Math.round(mb * 1024 * 1024);
};

// Computed properties for MB values
const fileCacheMaxSizePerFileMb = computed({
    get: () => (config.value?.core?.file_cache?.cache_max_size_per_file ? bytesToMb(config.value.core.file_cache.cache_max_size_per_file) : 0),
    set: (value) => {
        if (config.value?.core?.file_cache) {
            config.value.core.file_cache.cache_max_size_per_file = mbToBytes(value);
        }
    },
});

// Initialize
onMounted(() => {
    loadConfiguration();
});
</script>

<template>
    <div :class="inline ? 'config-editor-inline' : 'config-editor'">
        <!-- Header -->
        <div v-if="!inline" class="config-header">
            <h2>Configuration Editor</h2>
            <div class="config-actions">
                <button @click="saveConfiguration" class="save-button" :disabled="isSaving">
                    <span v-if="isSaving">Saving...</span>
                    <span v-else>Save Configuration</span>
                </button>
                <button v-if="hasUnsavedChanges" @click="resetChanges" class="reset-button" :disabled="isSaving">Reset Changes</button>
                <button @click="showReloadConfirmation" class="reload-button" :disabled="isReloading || isSaving">
                    <span v-if="isReloading">Reloading...</span>
                    <span v-else>Reload Configuration</span>
                </button>

                <button v-if="!inline" @click="emit('close')" class="close-button">Close</button>
            </div>
        </div>

        <!-- Loading State -->
        <div v-if="isLoading" class="loading-message">
            <div class="loading-spinner"></div>
            Loading configuration...
        </div>

        <!-- Error State -->
        <!-- Error State (for loading/configuration errors) -->
        <div v-else-if="error && !config" class="error-message">
            <pre v-if="error.includes('\\n')" class="error-details">{{ error }}</pre>
            <span v-else>{{ error }}</span>
            <button @click="loadConfiguration" class="retry-button">Retry</button>
        </div>

        <!-- Main Content -->
        <div v-else-if="config" class="config-form">
            <!-- Save Error message (validation/save errors) -->
            <div v-if="saveErrorMessage || (saveErrors && saveErrors.length > 0)" class="save-error-message">
                <div class="error-header">
                    <span class="error-icon">⚠️</span>
                    <strong>Configuration Save Failed</strong>
                </div>

                <ul v-if="saveErrors && saveErrors.length > 0" class="save-error-list">
                    <li v-for="(err, idx) in saveErrors" :key="idx">{{ err }}</li>
                </ul>
                <p class="error-help">Please fix the errors above and try saving again.</p>
            </div>

            <!-- Success message -->
            <div v-if="successMessage" class="success-message">
                {{ successMessage }}
            </div>

            <!-- Error message for save operations -->
            <div v-if="error && config" class="form-error-message">
                <pre v-if="error.includes('\\n')" class="error-details">{{ error }}</pre>
                <span v-else>{{ error }}</span>
            </div>

            <!-- Top Actions for Inline Mode -->
            <div v-if="inline" class="top-actions">
                <div class="top-buttons">
                    <button @click="saveConfiguration" class="save-button top" :disabled="isSaving">
                        <span v-if="isSaving">Saving...</span>
                        <span v-else>Save Configuration</span>
                    </button>
                    <button v-if="hasUnsavedChanges" @click="resetChanges" class="reset-button top" :disabled="isSaving">Reset Changes</button>
                    <button @click="showReloadConfirmation" class="reload-button top" :disabled="isReloading || isSaving">
                        <span v-if="isReloading">Reloading...</span>
                        <span v-else>Reload Config</span>
                    </button>

                    <!-- Unsaved changes indicator -->
                    <div v-if="hasUnsavedChanges" class="changes-indicator-top">You have unsaved changes</div>
                </div>
            </div>

            <!-- Network Bindings Section -->
            <div class="config-section">
                <div class="section-header" @click="toggleSection('bindings')">
                    <span class="section-icon" :class="{ expanded: expandedSections.bindings }">▶</span>
                    <span class="section-title-icon">🔌</span>
                    <h3>Network Bindings</h3>
                    <button @click.stop="addBinding" class="add-button">+ Add Binding</button>
                </div>

                <div v-if="expandedSections.bindings" class="section-content">
                    <div v-if="!config.bindings || config.bindings.length === 0" class="empty-state-section">
                        <div class="empty-icon">�</div>
                        <p>No network bindings configured</p>
                        <button @click="addBinding" class="add-button">+ Add First Binding</button>
                    </div>

                    <!-- Bindings List -->
                    <div v-for="(binding, bindingIndex) in config.bindings" :key="binding.id" class="server-item">
                        <div class="item-header compact" @click="toggleBinding(bindingIndex)">
                            <div class="header-left">
                                <span class="section-icon" :class="{ expanded: isBindingExpanded(bindingIndex) }">▶</span>
                                <span class="hierarchy-indicator binding-indicator">🔌</span>
                                <h4>{{ binding.ip }}:{{ binding.port }}</h4>
                                <span v-if="binding.is_admin" class="admin-badge">ADMIN</span>
                                <span v-if="binding.is_tls" class="tls-badge">TLS</span>
                            </div>
                            <button @click.stop="removeBinding(bindingIndex)" class="remove-button compact" :disabled="config.bindings.length === 1">Remove</button>
                        </div>

                        <!-- Binding Content -->
                        <div v-if="isBindingExpanded(bindingIndex)" class="item-content">
                            <div class="form-grid compact">
                                <div class="form-field checkbox-grid">
                                    <label>
                                        <input v-model="binding.is_tls" type="checkbox" />
                                        Enable TLS (https://)
                                        <span class="help-icon" data-tooltip="Enable this if you want to secure the connection using TLS. If you do, you should also specify the paths to the TLS certificate and key files on the sites attached.">?</span>
                                    </label>
                                    <label>
                                        <input v-model="binding.is_admin" type="checkbox" />
                                        Admin portal
                                        <span class="help-icon" data-tooltip="Whether this binding is for the Gruxi Admin portal, serving the API for admin requests. This should ONLY be enable on the binding which serves the admin interface.">?</span>
                                    </label>
                                </div>
                            </div>

                            <div class="two-column-layout form-grid max500">
                                <div class="compact half-width">
                                    <div class="form-field small-field">
                                        <label>IP Address</label>
                                        <input v-model="binding.ip" type="text" />
                                    </div>
                                </div>
                                <div class="compact half-width">
                                    <div class="form-field small-field">
                                        <label>Port</label>
                                        <input v-model.number="binding.port" type="number" min="1" max="65535" />
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <!-- Sites Section -->
            <div class="config-section">
                <div class="section-header" @click="toggleSection('sites')">
                    <span class="section-icon" :class="{ expanded: expandedSections.sites }">▶</span>
                    <span class="section-title-icon">🌐</span>
                    <h3>Sites</h3>
                    <button @click.stop="addSite" class="add-button">+ Add Site</button>
                </div>

                <div v-if="expandedSections.sites" class="section-content">
                    <div v-if="!config.sites || config.sites.length === 0" class="empty-state-section">
                        <div class="empty-icon">🌐</div>
                        <p>No sites configured</p>
                        <button @click="addSite" class="add-button">+ Add First Site</button>
                    </div>

                    <!-- Sites List -->
                    <div v-for="(site, siteIndex) in config.sites" :key="site.id" class="server-item">
                        <div class="item-header compact" @click="toggleSite(siteIndex)">
                            <div class="header-left">
                                <span class="section-icon" :class="{ expanded: isSiteExpanded(siteIndex) }">▶</span>
                                <span class="hierarchy-indicator site-indicator">🌐</span>
                                <h4 class="site-hostname-title">{{ site.hostnames.join(' - ') || 'No hostnames' }}</h4>
                                <span v-if="site.is_default" class="default-badge">DEFAULT</span>
                                <span v-if="!site.is_enabled" class="admin-badge">DISABLED</span>
                                <span v-if="getSiteBindings(site.id).some((bindingId) => config.bindings?.find((b) => b.id === bindingId)?.is_admin)" class="admin-badge">ADMIN PORTAL</span>
                            </div>
                            <button @click.stop="removeSite(siteIndex)" class="remove-button compact" :disabled="config.sites.length === 1">Remove</button>
                        </div>

                        <!-- Site Content -->
                        <div v-if="isSiteExpanded(siteIndex)" class="item-content">
                            <div class="form-grid compact">
                                <div class="form-field checkbox-grid compact">
                                    <label>
                                        <input v-model="site.is_enabled" type="checkbox" />
                                        Enabled
                                    </label>
                                    <label>
                                        <input v-model="site.is_default" type="checkbox" />
                                        Default Site
                                    </label>
                                    <label>
                                        <input v-model="site.access_log_enabled" type="checkbox" />
                                        Enable Access Logging
                                    </label>
                                </div>
                            </div>

                            <!-- Associated Network Bindings -->
                            <div class="form-grid compact">
                                <div class="form-field checkbox-grid compact">
                                    <label v-for="binding in getAvailableBindings()" :key="binding.id">
                                        <input type="checkbox" :checked="getSiteBindings(site.id).includes(binding.id)" @change="(e) => (e.target.checked ? associateSiteWithBinding(site.id, binding.id) : disassociateSiteFromBinding(site.id, binding.id))" />
                                        {{ binding.label }}
                                    </label>
                                </div>
                            </div>

                            <div v-if="getSiteBindings(site.id).length === 0" class="form-grid compact">
                                <div class="form-field">
                                    <div class="empty-association-warning-inline">⚠️ This site is not associated with any bindings and will not be accessible.</div>
                                </div>
                            </div>

                            <div v-if="site.access_log_enabled" class="form-grid compact">
                                <div class="form-field">
                                    <label>
                                        Access Log File
                                        <span class="help-icon" data-tooltip="Path to the access log file. If relative to gruxi base directory, use it like this './logs/mylog.log'. You can also have a full absolute path like '/var/logs/mylog.log' or 'C:/logs/mylog.log'.">?</span>
                                    </label>
                                    <input v-model="site.access_log_file" type="text" placeholder="Path to log file" />
                                </div>
                            </div>

                            <!-- Request Processing Section -->
                            <div class="request-processing-section">
                                <div class="subsection-header compact" @click="toggleSiteSubsection(siteIndex, 'requestProcessing')">
                                    <div class="header-left">
                                        <span class="section-icon" :class="{ expanded: isSiteSubsectionExpanded(siteIndex, 'requestProcessing') }">▶</span>
                                        <h5 class="subsection-title">Hostnames, Headers & Rewrites</h5>
                                    </div>
                                </div>

                                <div v-if="isSiteSubsectionExpanded(siteIndex, 'requestProcessing')" class="section-top-margin">
                                    <!-- Hostnames -->
                                    <div class="form-field">
                                        <div class="list-field compact">
                                            <label>Hostnames (use * to match all hostnames)</label>
                                            <div class="tag-field">
                                                <span v-for="(hostname, hostnameIndex) in site.hostnames" :key="hostnameIndex" class="tag-item">
                                                    {{ hostname }}
                                                    <button @click="removeHostname(siteIndex, hostnameIndex)" class="tag-remove-button" type="button">×</button>
                                                </span>
                                                <input
                                                    type="text"
                                                    class="tag-input"
                                                    placeholder="Add hostname..."
                                                    @keydown.enter.prevent="
                                                        (e) => {
                                                            if (e.target.value.trim()) {
                                                                addHostname(siteIndex);
                                                                site.hostnames[site.hostnames.length - 1] = e.target.value.trim();
                                                                e.target.value = '';
                                                            }
                                                        }
                                                    "
                                                />
                                            </div>
                                        </div>
                                    </div>

                                    <div class="two-column-layout">
                                        <div class="list-field compact half-width">
                                            <!-- Rewrite Functions -->
                                            <div class="form-field">
                                                <label>Rewrite Functions - Pre-defined request rewrites</label>
                                                <div class="doc-link">
                                                    <a href="https://gruxi.eu/docs/#rewrite-functions" target="_blank">Documentation on rewrite functions</a>
                                                </div>
                                                <div class="list-items">
                                                    <div v-for="(func, funcIndex) in site.rewrite_functions" :key="funcIndex" class="list-item">
                                                        <select v-model="site.rewrite_functions[funcIndex]" class="rewrite-select">
                                                            <option value="">-- Select Function --</option>
                                                            <option v-for="option in rewriteFunctionOptions" :key="option" :value="option">
                                                                {{ option }}
                                                            </option>
                                                        </select>
                                                        <button @click="removeRewriteFunction(siteIndex, funcIndex)" class="remove-item-button">×</button>
                                                    </div>
                                                    <button @click="addRewriteFunction(siteIndex)" class="add-item-button">+ Add Function</button>
                                                </div>
                                            </div>
                                        </div>
                                        <div class="list-field compact half-width">
                                            <!-- Extra Headers -->
                                            <div class="form-field">
                                                <label>Extra HTTP Headers</label>
                                                <div class="list-items">
                                                    <div v-for="(hdr, hdrIndex) in site.extra_headers || []" :key="hdrIndex" class="list-item key-value">
                                                        <input v-model="site.extra_headers[hdrIndex].key" type="text" placeholder="Header Key" class="key-input" />
                                                        <input v-model="site.extra_headers[hdrIndex].value" type="text" placeholder="Header Value" class="value-input" />
                                                        <button @click="removeExtraHeader(siteIndex, hdrIndex)" class="remove-item-button">×</button>
                                                    </div>
                                                    <button @click="addExtraHeader(siteIndex)" class="add-item-button">+ Add Header</button>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>

                            <!-- Processors Section -->
                            <div class="processors-section">
                                <div class="subsection-header compact" @click="toggleSiteSubsection(siteIndex, 'processors')">
                                    <div class="header-left">
                                        <span class="section-icon" :class="{ expanded: isSiteSubsectionExpanded(siteIndex, 'processors') }">▶</span>
                                        <h5 class="subsection-title">Processors ({{ getSiteProcessors(siteIndex).length }})</h5>
                                    </div>
                                    <div class="processors-toolbar">
                                        <button @click.stop="addProcessorToSite(siteIndex, 'static')" class="add-button small">+ Static Files</button>
                                        <button @click.stop="addProcessorToSite(siteIndex, 'php')" class="add-button small">+ PHP</button>
                                        <button @click.stop="addProcessorToSite(siteIndex, 'proxy')" class="add-button small">+ Proxy</button>
                                    </div>
                                </div>

                                <div v-if="isSiteSubsectionExpanded(siteIndex, 'processors')">
                                    <div v-if="getSiteProcessors(siteIndex).length === 0" class="empty-state-section small">
                                        <div class="empty-icon">⚙️</div>
                                        <p>No processors configured. This site will be unreachable.</p>
                                    </div>

                                    <!-- Processors List -->
                                    <div v-for="(processor, processorIndex) in getSiteProcessorModels(siteIndex)" :key="processor.handler.id" class="processor-item">
                                        <div class="item-header compact" @click="toggleSiteProcessor(siteIndex, processorIndex)">
                                            <div class="header-left">
                                                <span class="section-icon" :class="{ expanded: isSiteProcessorExpanded(siteIndex, processorIndex) }">▶</span>
                                                <span v-if="processor.handler.processor_type === 'static'" class="hierarchy-indicator">📄</span>
                                                <span v-else-if="processor.handler.processor_type === 'php'" class="hierarchy-indicator">🐘</span>
                                                <span v-else-if="processor.handler.processor_type === 'proxy'" class="hierarchy-indicator">🔀</span>
                                                <h6>{{ processor.handler.name || processor.handler.processor_type?.toUpperCase() + ' Processor' }}</h6>
                                                <span class="priority-badge">Priority: {{ processorIndex + 1 }}</span>
                                                <div class="priority-controls">
                                                    <button @click.stop="moveProcessorOrder(siteIndex, processor.handler.id, 'up')" class="priority-adjust-button" title="Move up">▲</button>
                                                    <button @click.stop="moveProcessorOrder(siteIndex, processor.handler.id, 'down')" class="priority-adjust-button" title="Move down">▼</button>
                                                </div>
                                                <span v-if="!processor.handler.is_enabled" class="admin-badge">DISABLED</span>
                                            </div>
                                            <button @click.stop="removeProcessorFromSite(siteIndex, processorIndex)" class="remove-button compact small">Remove</button>
                                        </div>

                                        <!-- Processor Content -->
                                        <div v-if="isSiteProcessorExpanded(siteIndex, processorIndex)" class="item-content">
                                            <div class="processor-config">
                                                <div class="form-grid compact">
                                                    <div class="form-field checkbox-grid compact">
                                                        <label>
                                                            <input v-model="processor.handler.is_enabled" type="checkbox" />
                                                            Enabled
                                                        </label>

                                                        <label v-if="processor.handler.processor_type === 'proxy'">
                                                            <input v-model="processor.proxy_config.verify_tls_certificates" type="checkbox" />
                                                            Verify TLS Certificates
                                                            <span class="help-icon" data-tooltip="If enabled, TLS certificates of the upstream server will be verified when proxying. For self-signed certificates, this should likely be disabled.">?</span>
                                                        </label>
                                                    </div>
                                                </div>

                                                <div class="form-grid compact">
                                                    <div class="form-field">
                                                        <label>Name <span class="help-icon" data-tooltip="The name of the processor, used for identification purposes only.">?</span></label>
                                                        <input v-model="processor.handler.name" type="text" placeholder="Processor name" />
                                                    </div>

                                                    <div class="form-field">
                                                        <label>URL Match Patterns <span class="help-icon" data-tooltip="List of url match patterns. * is used to match on all urls and means that this processor will try to serve all urls, if possible. You can add multiple patterns to match for this processor, such as '/assets' and '/static'.">?</span></label>
                                                        <div class="tag-field">
                                                            <div v-for="(pattern, patternIndex) in processor.handler.url_match" :key="patternIndex" class="tag-item">
                                                                {{ pattern }}
                                                                <button @click="removeUrlMatchFromProcessor(siteIndex, processorIndex, patternIndex)" class="tag-remove-button">×</button>
                                                            </div>
                                                            <input
                                                                @keydown.enter.prevent="
                                                                    addUrlMatchToProcessor(siteIndex, processorIndex, $event.target.value);
                                                                    $event.target.value = '';
                                                                "
                                                                type="text"
                                                                placeholder="Enter pattern and press Enter"
                                                                class="tag-input"
                                                            />
                                                        </div>
                                                    </div>

                                                    <!-- Processor type-specific configuration -->
                                                    <div v-if="processor.handler.processor_type === 'static'" class="form-field">
                                                        <div v-if="processor.static_config" class="processor-type-config form-grid">
                                                            <label>
                                                                Web Root
                                                                <span class="help-icon" data-tooltip="Directory to serve static files from, relative (./www-default) or absolute.">?</span>
                                                            </label>
                                                            <input v-model="processor.static_config.web_root" type="text" placeholder="./www-default" />

                                                            <div class="list-field compact">
                                                                <label>Index Files <span class="help-icon" data-tooltip="If a users requests a directory, Gruxi looks after these files to be served, such as 'index.html'.">?</span></label>
                                                                <div class="list-items">
                                                                    <div v-for="(file, fileIndex) in processor.static_config.web_root_index_file_list" :key="fileIndex" class="list-item">
                                                                        <input v-model="processor.static_config.web_root_index_file_list[fileIndex]" type="text" placeholder="index.html" />
                                                                        <button @click="processor.static_config.web_root_index_file_list.splice(fileIndex, 1)" class="remove-item-button">×</button>
                                                                    </div>
                                                                    <button @click="processor.static_config.web_root_index_file_list.push('index.html')" class="add-item-button">+ Add Index File</button>
                                                                </div>
                                                            </div>
                                                        </div>

                                                        <div v-else class="empty-association-warning-inline">⚠️ Static processor config not found for ID: {{ processor.handler.processor_id }}</div>
                                                    </div>

                                                    <div v-else-if="processor.handler.processor_type === 'php'" class="form-field">
                                                        <div v-if="processor.php_config" class="processor-type-config">
                                                            <label>Served By <span class="help-icon" data-tooltip="Select the PHP handler type used to serve PHP files. The Windows PHP-CGI option is mostly on Windows platforms and refers to a managed instance of PHP-CGI controlled by Gruxi. On Linux systems, PHP-FPM (FastCGI Process Manager) should be used.">?</span></label>
                                                            <select v-model="processor.php_config.served_by_type">
                                                                <option value="php-fpm">PHP-FPM (FastCGI)</option>
                                                                <option value="win-php-cgi">Windows PHP-CGI (managed)</option>
                                                            </select>

                                                            <div class="two-column-layout">
                                                                <div class="half-width">
                                                                    <label>
                                                                        Local Web Root
                                                                        <span class="help-icon" data-tooltip="Local web root Gruxi uses to resolve PHP files (Used by the local managed PHP-CGI mode as the web root).">?</span>
                                                                    </label>
                                                                    <input v-model="processor.php_config.local_web_root" type="text" placeholder="./www-default" />
                                                                </div>
                                                                <div class="half-width">
                                                                    <label>Request Timeout (seconds) <span class="help-icon" data-tooltip="Request timeout in seconds for PHP requests. This supersedes the request timeout defined in php.ini file and this should be set equal or a bit higher than the max_execution_time setting in php.ini.">?</span></label>
                                                                    <input v-model.number="processor.php_config.request_timeout" type="number" min="1" max="3600" />
                                                                </div>
                                                            </div>

                                                            <div v-if="processor.php_config.served_by_type === 'php-fpm'" class="two-column-layout">
                                                                <div class="half-width">
                                                                    <label>FastCGI IP:Port <span class="help-icon" data-tooltip="IP address and port of the FastCGI server, if using the PHP-FPM mode (e.g., 127.0.0.1:9000)">?</span></label>
                                                                    <input v-model="processor.php_config.fastcgi_ip_and_port" type="text" placeholder="127.0.0.1:9000" />
                                                                </div>
                                                                <div class="half-width">
                                                                    <label>FastCGI Web Root <span class="help-icon" data-tooltip="Web root directory used by the FastCGI server, if using the PHP-FPM mode (e.g., /var/www/html). File request paths will be rewritten to match this root.">?</span></label>
                                                                    <input v-model="processor.php_config.fastcgi_web_root" type="text" placeholder="/var/www/html" />
                                                                </div>
                                                            </div>

                                                            <div v-else-if="processor.php_config.served_by_type === 'win-php-cgi'" class="form-field">
                                                                <label>PHP-CGI Handler <span class="help-icon" data-tooltip="Select the PHP-CGI handler to use for processing PHP requests in the Windows PHP-CGI mode.">?</span></label>
                                                                <select v-if="config.php_cgi_handlers && config.php_cgi_handlers.length" v-model="processor.php_config.php_cgi_handler_id">
                                                                    <option value="">-- Select Handler --</option>
                                                                    <option v-for="h in config.php_cgi_handlers" :key="h.id" :value="h.id">{{ (h.name && String(h.name).trim().length ? h.name : 'PHP-CGI') + ' (' + h.id + ')' }}</option>
                                                                </select>
                                                                <input v-else v-model="processor.php_config.php_cgi_handler_id" type="text" placeholder="PHP-CGI handler UUID" />
                                                            </div>
                                                        </div>
                                                        <div v-else class="empty-association-warning-inline">⚠️ PHP processor config not found for ID: {{ processor.handler.processor_id }}</div>
                                                    </div>

                                                    <div v-else-if="processor.handler.processor_type === 'proxy'" class="form-field">
                                                        <div v-if="processor.proxy_config" class="processor-type-config">
                                                            <div class="two-column-layout">
                                                                <div class="half-width">
                                                                    <label>Proxy Type <span class="help-icon" data-tooltip="Select the type of proxy to use for this processor. Currently only HTTP is supported.">?</span></label>
                                                                    <select v-model="processor.proxy_config.proxy_type">
                                                                        <option value="http">HTTP</option>
                                                                    </select>
                                                                </div>
                                                                <div class="half-width">
                                                                    <label>Load Balancing Strategy <span class="help-icon" data-tooltip="Select the strategy used to distribute requests among upstream servers. Currently only Round Robin is supported.">?</span></label>
                                                                    <select v-model="processor.proxy_config.load_balancing_strategy">
                                                                        <option value="round_robin">Round Robin</option>
                                                                    </select>
                                                                </div>
                                                            </div>

                                                            <div class="form-field">
                                                                <label>Upstream Servers <span class="help-icon" data-tooltip="List of upstream servers to which requests will be proxied, in the form: 'http://hostname:port' or 'https://hostname:port'.">?</span></label>
                                                                <div class="list-items">
                                                                    <div v-for="(server, serverIndex) in processor.proxy_config.upstream_servers" :key="serverIndex" class="list-item">
                                                                        <input v-model="processor.proxy_config.upstream_servers[serverIndex]" type="text" placeholder="http://localhost:8080" />
                                                                        <button @click="processor.proxy_config.upstream_servers.splice(serverIndex, 1)" class="remove-item-button">×</button>
                                                                    </div>
                                                                    <button @click="processor.proxy_config.upstream_servers.push('http://localhost:8080')" class="add-item-button">+ Add Upstream</button>
                                                                </div>
                                                            </div>

                                                            <div class="two-column-layout">
                                                                <div class="half-width">
                                                                    <label>Timeout (seconds) <span class="help-icon" data-tooltip="Timeout, in seconds, for proxy requests to upstream server before connection is considered failed.">?</span></label>
                                                                    <input v-model.number="processor.proxy_config.timeout_seconds" type="number" min="1" max="3600" />
                                                                </div>
                                                                <div class="half-width"></div>
                                                            </div>

                                                            <div class="two-column-layout">
                                                                <div class="half-width">
                                                                    <label>
                                                                        Health Check Path (empty = disabled)
                                                                        <span class="help-icon" data-tooltip="Path to check on upstream server for health status. Only checks for HTTP 200 OK response and content is ignored. Example: '/health' or '/' or '/health?key=123'. To disable, leave the field empty.">?</span>
                                                                    </label>
                                                                    <input v-model="processor.proxy_config.health_check_path" type="text" placeholder="/health" />
                                                                </div>
                                                                <div class="half-width">
                                                                    <label>Health Check Interval (seconds) <span class="help-icon" data-tooltip="Interval, in seconds, between health checks to upstream server.">?</span></label>
                                                                    <input v-model.number="processor.proxy_config.health_check_interval_seconds" type="number" min="1" max="86400" />
                                                                </div>
                                                            </div>
                                                            <div class="two-column-layout">
                                                                <div class="half-width">
                                                                    <label>Health Check Timeout (seconds) <span class="help-icon" data-tooltip="Timeout, in seconds, for each health check request to upstream server. Should be kept relatively low, like 5 seconds.">?</span></label>
                                                                    <input v-model.number="processor.proxy_config.health_check_timeout_seconds" type="number" min="1" max="3600" />
                                                                </div>
                                                                <div class="half-width"></div>
                                                            </div>

                                                            <div class="list-field compact">
                                                                <label>URL Rewrites <span class="help-icon" data-tooltip="Define rules to rewrite URLs before they are sent to the upstream server, which is done as a search/replace in the full url, considering whether it should be case sensitive or not.">?</span></label>
                                                                <div class="list-items">
                                                                        <div v-for="(rw, rwIndex) in processor.proxy_config.url_rewrites" :key="rwIndex" class="list-item url-rewrite-item">
                                                                        <div class="rewrite-row">
                                                                            <div class="rewrite-field">
                                                                                <label class="rewrite-label">From:</label>
                                                                                <input v-model="rw.from" type="text" placeholder="/path" class="key-input" />
                                                                            </div>
                                                                            <div class="rewrite-field">
                                                                                <label class="rewrite-label">To:</label>
                                                                                <input v-model="rw.to" type="text" placeholder="/new-path" class="value-input" />
                                                                            </div>
                                                                                <button @click="processor.proxy_config.url_rewrites.splice(rwIndex, 1)" class="remove-item-button rewrite-remove-button">×</button>
                                                                        </div>
                                                                        <label class="inline-checkbox">
                                                                            <input v-model="rw.is_case_insensitive" type="checkbox" />
                                                                            Is case insensitive? <span class="help-icon" data-tooltip="If enabled, the URL rewrite rule will be applied in a case-insensitive manner. Eg. if enabled, the rewrite '/API' to '/my-site-api' will also be applied to '/api', '/Api' and changed to '/my-site-api'. If not enabled, only the exact case will be matched.">?</span>
                                                                        </label>

                                                                    </div>
                                                                    <button @click="processor.proxy_config.url_rewrites.push({ from: '', to: '', is_case_insensitive: false })" class="add-item-button">+ Add Rewrite</button>
                                                                </div>
                                                            </div>

                                                            <div class="form-grid compact">
                                                                <div class="form-field checkbox-grid compact">
                                                                    <label>
                                                                        <input v-model="processor.proxy_config.preserve_host_header" type="checkbox" />
                                                                        Preserve Host Header
                                                                        <span class="help-icon" data-tooltip="If enabled, the original Host header from the client request will be preserved when proxying to the upstream server.">?</span>
                                                                    </label>
                                                                </div>
                                                                <div class="form-field">
                                                                    <label>Forced Host Header (optional) <span class="help-icon" data-tooltip="If set, this value will be used as the Host header when proxying to the upstream server, overriding the original Host header from the client request. Will break HTTP2 requests and should probably in most cases not be used.">?</span></label>
                                                                    <input v-model="processor.proxy_config.forced_host_header" type="text" placeholder="example.com" />
                                                                </div>
                                                            </div>
                                                        </div>
                                                        <div v-else class="empty-association-warning-inline">⚠️ Proxy processor config not found for ID: {{ processor.handler.processor_id }}</div>
                                                    </div>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>

                            <!-- TLS Certificate Settings -->
                            <div class="tls-settings-section">
                                <div class="subsection-header compact" @click="toggleSiteSubsection(siteIndex, 'tls')">
                                    <div class="header-left">
                                        <span class="section-icon" :class="{ expanded: isSiteSubsectionExpanded(siteIndex, 'tls') }">▶</span>
                                        <h5 class="subsection-title">TLS Certificate Settings</h5>
                                    </div>
                                </div>

                                <div v-if="isSiteSubsectionExpanded(siteIndex, 'tls')">
                                    <div class="info-field">
                                        <p><strong>Note:</strong> You can either specify file paths or paste the certificate/key content directly. If both are provided, the file paths take precedence.</p>
                                    </div>
                                    <div class="tls-grid-full">
                                        <div class="tls-paths-row">
                                            <div class="form-field">
                                                <label
                                                    >Certificate Path
                                                    <span class="help-icon" data-tooltip="Path to the TLS certificate file. You can specify an absolute path or a path relative to the Gruxi server base directory. Such as './certs/mycert.pem' or '/etc/ssl/certs/mycert.pem'.">?</span>
                                                </label>
                                                <input v-model="site.tls_cert_path" type="text" placeholder="Path to certificate file" />
                                            </div>
                                            <div class="form-field">
                                                <label
                                                    >Private Key Path
                                                    <span class="help-icon" data-tooltip="Path to the TLS private key file. You can specify an absolute path or a path relative to the Gruxi server base directory. Such as './certs/mykey.pem' or '/etc/ssl/private/mykey.pem'.">?</span>
                                                </label>
                                                <input v-model="site.tls_key_path" type="text" placeholder="Path to private key file" />
                                            </div>
                                        </div>
                                        <div class="tls-content-row">
                                            <div class="form-field">
                                                <label>Certificate Content (PEM format)</label>
                                                <textarea v-model="site.tls_cert_content" placeholder="Paste your certificate content here in PEM format (-----BEGIN CERTIFICATE-----...)" rows="6" class="tls-content-textarea"></textarea>
                                            </div>
                                            <div class="form-field">
                                                <label>Private Key Content (PEM format)</label>
                                                <textarea v-model="site.tls_key_content" placeholder="Paste your private key content here in PEM format (-----BEGIN PRIVATE KEY-----...)" rows="6" class="tls-content-textarea"></textarea>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <!-- Managed External Systems Section -->
            <div class="config-section">
                <div class="section-header" @click="toggleSection('managedExternalSystems')">
                    <span class="section-icon" :class="{ expanded: expandedSections.managedExternalSystems }">▶</span>
                    <span class="section-title-icon">🔌</span>
                    <h3>Managed External Systems</h3>
                    <button @click.stop="addPhpCgiHandler" class="add-button">+ Add System</button>
                </div>

                <div v-if="expandedSections.managedExternalSystems" class="section-content">
                    <div v-if="!config.php_cgi_handlers || config.php_cgi_handlers.length === 0" class="empty-state-section">
                        <div class="empty-icon">🔌</div>
                        <p>No managed external systems configured</p>
                        <button @click="addPhpCgiHandler" class="add-button">+ Add First System</button>
                    </div>

                    <!-- Managed External Systems List (PHP-CGI only for now) -->
                    <div v-for="(handler, handlerIndex) in config.php_cgi_handlers" :key="handler.id" class="server-item">
                        <div class="item-header compact" @click="togglePhpCgiHandler(handlerIndex)">
                            <div class="header-left">
                                <span class="section-icon" :class="{ expanded: isPhpCgiHandlerExpanded(handlerIndex) }">▶</span>
                                <span class="hierarchy-indicator handler-indicator">🔌</span>
                                <h4>{{ handler.name || 'PHP-CGI' }}</h4>
                                <span class="handler-type-badge">PHP-CGI</span>
                                <span class="item-summary">{{ handler.id }}</span>
                            </div>
                            <button @click.stop="removePhpCgiHandler(handlerIndex)" class="remove-button compact">Remove</button>
                        </div>

                        <!-- Managed External System Content -->
                        <div v-if="isPhpCgiHandlerExpanded(handlerIndex)" class="item-content">
                            <div class="form-grid compact">
                                <div class="form-field">
                                    <label>Name</label>
                                    <input v-model="handler.name" type="text" placeholder="e.g., PHP-CGI Pool" />
                                </div>
                                <div class="form-field">
                                    <label>
                                        Executable Path
                                        <span class="help-icon" data-tooltip="Full path to php-cgi.exe. This must exist on the server running Gruxi.">?</span>
                                    </label>
                                    <input v-model="handler.executable" type="text" placeholder="C:/path/to/php-cgi.exe" />
                                </div>
                                <div class="form-field">
                                    <label>Request Timeout (seconds)</label>
                                    <input v-model.number="handler.request_timeout" type="number" min="1" max="3600" />
                                </div>
                                <div class="form-field">
                                    <label>Concurrent Threads (0 = auto)</label>
                                    <input v-model.number="handler.concurrent_threads" type="number" min="0" max="1000" />
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <!-- Core Settings Section -->
            <div class="config-section">
                <div class="section-header" @click="toggleSection('core')">
                    <span class="section-icon" :class="{ expanded: expandedSections.core }">▶</span>
                    <span class="section-title-icon">⚡</span>
                    <h3>Core Settings</h3>
                </div>

                <div v-if="expandedSections.core" class="section-content">
                    <!-- File Cache Settings -->
                    <div class="binding-item">
                        <div class="item-header compact" @click="toggleCoreSubsection('fileCache')">
                            <div class="header-left">
                                <span class="section-icon" :class="{ expanded: isCoreSubsectionExpanded('fileCache') }">▶</span>
                                <span class="hierarchy-indicator">📁</span>
                                <h4>File Cache</h4>
                                <span v-if="config.core.file_cache.is_enabled" class="default-badge">ENABLED</span>
                                <span v-else class="admin-badge">DISABLED</span>
                            </div>
                        </div>

                        <div v-if="isCoreSubsectionExpanded('fileCache')" class="item-content">
                            <div class="form-grid compact">
                                <div class="form-field full-width">
                                    <label>
                                        <input v-model="config.core.file_cache.is_enabled" type="checkbox" />
                                        Enable File Caching
                                    </label>
                                </div>
                                <div class="form-field">
                                    <label>Max Cached Items (count)</label>
                                    <input v-model.number="config.core.file_cache.cache_item_size" type="number" min="1" />
                                </div>
                                <div class="form-field">
                                    <label>Max Size Per File (MB)</label>
                                    <input v-model.number="fileCacheMaxSizePerFileMb" type="number" min="0" step="0.01" />
                                </div>
                                <div class="form-field">
                                    <label>How often to check files for changes (seconds)</label>
                                    <input v-model.number="config.core.file_cache.cache_item_time_between_checks" type="number" min="1" />
                                </div>
                                <div class="form-field">
                                    <label>Cleanup Thread Interval (seconds)</label>
                                    <input v-model.number="config.core.file_cache.cleanup_thread_interval" type="number" min="1" />
                                </div>
                                <div class="form-field">
                                    <label>Max Time To Keep a File (seconds)</label>
                                    <input v-model.number="config.core.file_cache.max_item_lifetime" type="number" min="0" />
                                </div>
                                <div class="form-field">
                                    <label>Forced Eviction Threshold (%)</label>
                                    <input v-model.number="config.core.file_cache.forced_eviction_threshold" type="number" min="1" max="99" />
                                </div>
                            </div>
                        </div>
                    </div>

                    <!-- Gzip Settings -->
                    <div class="binding-item">
                        <div class="item-header compact" @click="toggleCoreSubsection('gzip')">
                            <div class="header-left">
                                <span class="section-icon" :class="{ expanded: isCoreSubsectionExpanded('gzip') }">▶</span>
                                <span class="hierarchy-indicator">📦</span>
                                <h4>Gzip Compression</h4>
                                <span v-if="config.core.gzip.is_enabled" class="default-badge">ENABLED</span>
                                <span v-else class="admin-badge">DISABLED</span>
                                <span class="item-summary">({{ config.core.gzip.compressible_content_types?.length || 0 }} content types)</span>
                            </div>
                        </div>

                        <div v-if="isCoreSubsectionExpanded('gzip')" class="item-content">
                            <div class="form-grid compact">
                                <div class="form-field full-width">
                                    <label>
                                        <input v-model="config.core.gzip.is_enabled" type="checkbox" />
                                        Enable Gzip Compression
                                    </label>
                                </div>
                                <div class="form-field full-width">
                                    <label>Compressible Content Types</label>
                                    <div class="array-field">
                                        <div v-for="(contentType, index) in config.core.gzip.compressible_content_types" :key="index" class="array-item">
                                            <input v-model="config.core.gzip.compressible_content_types[index]" type="text" />
                                            <button @click="removeGzipContentType(index)" type="button" class="remove-button" title="Remove Content Type">✕</button>
                                        </div>
                                        <button @click="addGzipContentType" type="button" class="add-button">+ Add Content Type</button>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <!-- Reload Configuration Modal -->
        <div v-if="showReloadModal" class="modal-overlay" @click="hideReloadConfirmation">
            <div class="modal-content" @click.stop>
                <div class="modal-header">
                    <h3>Reload Configuration</h3>
                    <button @click="hideReloadConfirmation" class="modal-close-button">×</button>
                </div>

                <div class="modal-body">
                    <div v-if="reloadError" class="modal-error">
                        <div class="error-icon">❌</div>
                        <div>
                            <strong>Reload Failed</strong>
                            <p>{{ reloadError }}</p>
                        </div>
                    </div>

                    <div v-else class="modal-warning">
                        <div class="warning-icon">⚠️</div>
                        <div>
                            <strong>Are you sure you want to reload the configuration?</strong>
                            <p>This action will:</p>
                            <ul>
                                <li>Restart the server with the current saved configuration</li>
                                <li>Disconnect all active connections</li>
                            </ul>
                            <p class="warning-note">Make sure you have saved your configuration changes before reloading!</p>
                        </div>
                    </div>
                </div>

                <div class="modal-footer">
                    <div v-if="reloadError" class="modal-actions-error">
                        <button @click="hideReloadConfirmation" class="modal-button secondary">Close</button>
                        <button
                            @click="
                                () => {
                                    reloadError = '';
                                    confirmReloadConfiguration();
                                }
                            "
                            class="modal-button danger"
                            :disabled="isReloading"
                        >
                            <span v-if="isReloading">Reloading...</span>
                            <span v-else>Try Again</span>
                        </button>
                    </div>
                    <div v-else class="modal-actions">
                        <button @click="hideReloadConfirmation" class="modal-button secondary">Cancel</button>
                        <button @click="confirmReloadConfiguration" class="modal-button danger" :disabled="isReloading">
                            <span v-if="isReloading">Reloading...</span>
                            <span v-else>Reload Configuration</span>
                        </button>
                    </div>
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped>
.config-editor {
    background: #ffffff;
    border-radius: 12px;
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.1);
    border: 1px solid #e5e7eb;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.config-editor-inline {
    background: transparent;
    border: none;
    box-shadow: none;
    overflow: visible;
    max-height: none;
    display: flex;
    flex-direction: column;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

/* New visual hierarchy styles */
.section-title-icon {
    font-size: 1.25rem;
    margin-right: 0.5rem;
}

.hierarchy-indicator {
    font-size: 1rem;
    margin-right: 0.75rem;
    opacity: 0.8;
}

.header-left {
    display: flex;
    align-items: center;
    gap: 0.5rem;
}

.max500 {
    max-width: 500px;
}

.binding-summary,
.site-summary {
    font-size: 0.75rem;
    color: #6b7280;
    background: #f3f4f6;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    font-family: monospace;
}

.admin-badge,
.default-badge,
.tls-badge {
    font-size: 0.65rem;
    font-weight: 700;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}

.admin-badge {
    background: #ddd6fe;
    color: #6b21a8;
}

.default-badge {
    background: #dbeafe;
    color: #1d4ed8;
}

.tls-badge {
    background: #d1fae5;
    color: #065f46;
}

.empty-state-section {
    text-align: center;
    padding: 2rem 1.5rem;
    color: #6b7280;
    background: #f9fafb;
    margin: 1rem;
    border-radius: 8px;
    border: 2px dashed #e5e7eb;
}

.empty-state-section.small {
    padding: 1.5rem 1rem;
    margin: 0.75rem;
}

.empty-state-section .empty-icon {
    font-size: 2rem;
    margin-bottom: 0.75rem;
}

.empty-state-section.small .empty-icon {
    font-size: 1.5rem;
    margin-bottom: 0.5rem;
}

.empty-state-section p {
    margin: 0 0 1rem 0;
    font-weight: 500;
}

/* Compact styles */
.item-header.compact {
    padding: 0.1rem 1rem;
    cursor: pointer;
    min-height: 40px;
}

.item-header.compact:hover {
    background: linear-gradient(135deg, #f1f5f9 0%, #e2e8f0 100%);
}

.subsection-header.compact {
    margin: 1rem 1rem 0.75rem 1rem;
    padding-bottom: 0.375rem;
}

.form-grid.compact {
    padding: 1rem;
    gap: 1rem;
    width: fit-content;
}

.form-field.checkbox-grid.compact {
    padding: 0.75rem;
    gap: 3rem;
}

.form-field select {
    margin-bottom: 1rem;
}

.list-field.compact {
    margin: 0;
    padding: 1rem;
}

.remove-button.compact {
    padding: 0.2rem 0.75rem;
    font-size: 0.8rem;
}

.remove-button.compact.small {
    padding: 0.175rem 0.625rem;
    font-size: 0.75rem;
    min-height: 30px;
}

.item-summary {
    font-size: 0.75rem;
    color: #6b7280;
    background: #f9fafb;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    font-weight: 500;
}

.item-content {
    background: #fefefe;
    border-top: 1px solid #f1f5f9;
    padding-bottom: 20px;
}

.config-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1.5rem 2rem;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: white;
    border-bottom: none;
}

.inline-config-header {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    padding: 0 0 1rem 0;
    background: transparent;
    margin-bottom: 1rem;
}

.config-header h2 {
    margin: 0;
    color: white;
    font-size: 1.5rem;
    font-weight: 700;
}

.config-actions {
    display: flex;
    gap: 1rem;
    align-items: center;
}

.inline-config-actions {
    display: flex;
    gap: 0.75rem;
    align-items: center;
}

.save-button {
    padding: 0.75rem 1.5rem;
    background: linear-gradient(135deg, #10b981, #059669);
    color: white;
    border: none;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    box-shadow: 0 2px 4px rgba(16, 185, 129, 0.2);
}

.save-button.inline {
    padding: 0.625rem 1.25rem;
    font-size: 0.875rem;
    box-shadow: 0 1px 3px rgba(16, 185, 129, 0.2);
    background: linear-gradient(135deg, #059669, #047857);
}

.save-button:disabled {
    background: #9ca3af;
    cursor: not-allowed;
    box-shadow: none;
}

.save-button:not(:disabled):hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 16px rgba(16, 185, 129, 0.3);
}

.save-button.inline:not(:disabled):hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(16, 185, 129, 0.2);
}

.reset-button {
    padding: 0.75rem 1.5rem;
    background: #f59e0b;
    color: white;
    border: none;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    box-shadow: 0 2px 4px rgba(245, 158, 11, 0.2);
}

.reset-button.inline {
    padding: 0.625rem 1.25rem;
    font-size: 0.875rem;
    box-shadow: 0 1px 3px rgba(245, 158, 11, 0.2);
    background: #d97706;
}

.reset-button.inline:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 8px rgba(245, 158, 11, 0.2);
}

.reset-button:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 8px rgba(245, 158, 11, 0.3);
}

.reload-button {
    padding: 0.75rem 1.5rem;
    background: linear-gradient(135deg, #8b5cf6, #7c3aed);
    color: white;
    border: none;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    box-shadow: 0 2px 4px rgba(139, 92, 246, 0.2);
}

.reload-button.top {
    padding: 0.625rem 1.25rem;
    font-size: 0.875rem;
    box-shadow: 0 1px 3px rgba(139, 92, 246, 0.2);
    background: linear-gradient(135deg, #7c3aed, #6d28d9);
}

.reload-button:disabled {
    background: #9ca3af;
    cursor: not-allowed;
    box-shadow: none;
}

.reload-button:not(:disabled):hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 16px rgba(139, 92, 246, 0.3);
}

.reload-button.top:not(:disabled):hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(139, 92, 246, 0.2);
}

.close-button {
    padding: 0.75rem 1.5rem;
    background: rgba(255, 255, 255, 0.2);
    color: white;
    border: 1px solid rgba(255, 255, 255, 0.3);
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
}

.close-button:hover {
    background: rgba(255, 255, 255, 0.3);
    transform: translateY(-1px);
}

.config-form {
    flex: 1;
    overflow-y: auto;
    padding: 0;
    background: transparent;
}

.config-editor .config-form {
    padding: 1.5rem;
    background: #f8fafc;
}

.success-message {
    background: #ecfdf5;
    border-left: 4px solid #10b981;
    color: #065f46;
    padding: 1rem 1.5rem;
    border-radius: 8px;
    margin-bottom: 1.5rem;
    font-weight: 500;
}

.form-error-message {
    background: #fef2f2;
    border-left: 4px solid #ef4444;
    color: #dc2626;
    padding: 1rem 1.5rem;
    border-radius: 8px;
    margin-bottom: 1.5rem;
    font-weight: 500;
}

.save-error-message {
    background: #fef2f2;
    border: 2px solid #ef4444;
    color: #dc2626;
    padding: 1.5rem;
    border-radius: 12px;
    margin-bottom: 1.5rem;
    box-shadow: 0 4px 12px rgba(239, 68, 68, 0.1);
}

.save-error-message .error-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 1rem;
    font-size: 1.1rem;
}

.save-error-message .error-icon {
    font-size: 1.5rem;
}

.save-error-message .error-help {
    margin-top: 1rem;
    margin-bottom: 0;
    font-style: italic;
    color: #b91c1c;
    font-size: 0.9rem;
}

.save-error-list {
    margin: 0.75rem 0 0 1.25rem;
    padding: 0;
}

.save-error-list li {
    margin: 0.25rem 0;
}

.form-error-message .error-details,
.save-error-message .error-details {
    margin: 0.5rem 0 0 0;
    font-family: inherit;
    white-space: pre-wrap;
    line-height: 1.5;
    background: rgba(0, 0, 0, 0.05);
    padding: 0.75rem;
    border-radius: 6px;
    font-size: 0.875rem;
}

.changes-indicator {
    background: #fffbeb;
    border-left: 4px solid #f59e0b;
    color: #92400e;
    padding: 1rem 1.5rem;
    border-radius: 8px;
    margin-bottom: 1.5rem;
    text-align: left;
    font-weight: 600;
}

.config-section {
    margin-bottom: 1.5rem;
    background: white;
    border-radius: 12px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
    border: 1px solid #e5e7eb;
}

.config-editor-inline .config-section {
    background: white;
    border-radius: 8px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.02);
    border: 1px solid #f1f5f9;
    margin-bottom: 1rem;
}

.config-editor-inline .config-section:last-child {
    margin-bottom: 0;
}

.section-header {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 1rem 1.5rem;
    background: #ffffff;
    cursor: pointer;
    border-bottom: 1px solid #f1f5f9;
    transition: all 0.2s ease;
}

.section-header:hover {
    background: #f8fafc;
}

.section-icon {
    transition: transform 0.3s ease;
    font-size: 0.75rem;
    color: #667eea;
    font-weight: bold;
}

.section-icon.expanded {
    transform: rotate(90deg);
}

.section-header h3 {
    margin: 0;
    flex: 1;
    color: #1e293b;
    font-size: 1.125rem;
    font-weight: 700;
}

.section-content {
    padding: 0;
    background: #fafbfc;
}

.section-top-margin {
    margin-top: 20px;
}

.server-item {
    margin: 1rem;
    background: white;
    border-radius: 8px;
    border-left: 4px solid #3b82f6;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.05);
}

.binding-item {
    margin: 0.75rem 1rem;
    background: #f8fafc;
    border-radius: 8px;
    border-left: 3px solid #10b981;
}

.site-item {
    margin: 0.5rem 0.75rem;
    background: #ffffff;
    border-radius: 6px;
    border-left: 2px solid #f59e0b;
}

.item-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.25rem;
    background: linear-gradient(135deg, #f8fafc 0%, #e2e8f0 100%);
    border-bottom: 1px solid #e2e8f0;
}

.server-item .item-header {
    background: linear-gradient(135deg, #eff6ff 0%, #dbeafe 100%);
    border-left: 4px solid #3b82f6;
    margin-left: -4px;
}

.binding-item .item-header {
    background: linear-gradient(135deg, #f0fdf4 0%, #dcfce7 100%);
    border-left: 3px solid #10b981;
    margin-left: -3px;
}

.site-item .item-header {
    background: linear-gradient(135deg, #fffbeb 0%, #fef3c7 100%);
    border-left: 2px solid #f59e0b;
    margin-left: -2px;
}

.item-header h4,
.item-header h5,
.item-header h6 {
    margin: 0;
    font-weight: 700;
}

.item-header h4 {
    color: #1e40af;
    font-size: 1.1rem;
}

.item-header h5 {
    color: #047857;
    font-size: 1rem;
}

.item-header h6 {
    color: #92400e;
    font-size: 0.95rem;
}

.subsection-header {
    display: flex;
    align-items: center;
    margin: 1rem 1.25rem 0.75rem 1.25rem;
    padding-bottom: 0.375rem;
    border-bottom: 2px solid #e2e8f0;
}

.subsection-header.compact {
    cursor: pointer;
    padding: 0.75rem 1rem;
    margin: 0;
    border-radius: 6px;
    transition: background 0.2s ease;
    border: 0;
}

.subsection-header.compact:hover {
    background: #f8fafc;
}

.subsection-content {
    padding: 1rem 1.25rem;
}

/* Tag Field Styles */
.tag-field {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    padding: 0.75rem;
    border: 2px solid #e2e8f0;
    border-radius: 8px;
    background: #f8fafc;
    min-height: 48px;
    align-items: center;
    transition: all 0.2s ease;
}

.tag-field:focus-within {
    border-color: #3b82f6;
    background: white;
    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
}

.tag-item {
    display: inline-flex;
    align-items: center;
    gap: 1rem;
    padding: 0.375rem 0.625rem;
    background: linear-gradient(135deg, #3b82f6 0%, #2563eb 100%);
    color: white;
    border-radius: 6px;
    font-size: 0.875rem;
    font-weight: 500;
    line-height: 1.25;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    transition: all 0.2s ease;
}

.tag-item:hover {
    transform: translateY(-1px);
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.15);
}

.tag-remove-button {
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.2);
    border: none;
    color: white;
    cursor: pointer;
    padding: 0.125rem;
    width: 18px;
    height: 18px;
    border-radius: 3px;
    font-size: 0.875rem;
    line-height: 1;
    transition: all 0.2s ease;
}

.tag-remove-button:hover {
    background: rgba(255, 255, 255, 0.3);
    transform: scale(1.1);
}

.tag-input {
    flex: 1;
    min-width: 150px;
    border: none;
    outline: none;
    padding: 0.375rem 0.5rem;
    font-size: 0.875rem;
    background: transparent;
    color: #1e293b;
}

.tag-input::placeholder {
    color: #94a3b8;
}

.subsection-header h5,
.subsection-header h6 {
    margin: 0;
    color: #475569;
    font-weight: 600;
    font-size: 0.95rem;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}

.add-button {
    padding: 0.5rem 1rem;
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    color: white;
    border: none;
    border-radius: 6px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    font-size: 0.875rem;
    box-shadow: 0 1px 3px rgba(59, 130, 246, 0.3);
}

.add-button:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(59, 130, 246, 0.4);
}

.add-button.small {
    padding: 0.275rem 0.55rem;
    font-size: 12px;
}

.remove-button {
    background: linear-gradient(135deg, #ef4444, #dc2626);
    color: white;
    border: none;
    border-radius: 6px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    font-size: 0.875rem;
    padding: 0.5rem 1rem;
    box-shadow: 0 1px 3px rgba(239, 68, 68, 0.3);
}

.remove-button:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(239, 68, 68, 0.4);
}

.remove-button.small {
    padding: 0.375rem 0.75rem;
    font-size: 0.8rem;
}

.remove-button:disabled {
    background: #9ca3af;
    cursor: not-allowed;
    opacity: 0.5;
}

.remove-button:disabled:hover {
    transform: none;
    box-shadow: none;
}

.form-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(600px, 1fr));
    gap: 1.5rem;
    background: white;
}

.form-field {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
}

.form-field.small-field {
    min-width: 140px;
    max-width: 180px;
}

.form-field.full-width {
    grid-column: 1 / -1;
}

.form-field.half-width {
    min-width: 200px;
}

.form-field.checkbox-grid {
    display: flex;
    flex-wrap: wrap;
    flex-direction: row;
    gap: 2rem;
    padding: 1rem;
    background: #f8fafc;
    border-radius: 8px;
    border: 1px solid #e2e8f0;
}

.form-field label {
    font-weight: 600;
    color: #374151;
    display: flex;
    align-items: center;
    font-size: 0.875rem;
    margin-bottom: 0.25rem;
}

.form-field input[type='text'],
.form-field input[type='number'] {
    padding: 0.375rem;
    border: 2px solid #e5e7eb;
    border-radius: 8px;
    font-size: 0.875rem;
    transition: all 0.2s ease;
    background: white;
}

.form-field input[type='text']:focus,
.form-field input[type='number']:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.form-field input[type='checkbox'] {
    margin: 0;
    width: 18px;
    height: 18px;
    accent-color: #667eea;
    margin-right: 10px;
}

.list-field {
    margin: 1rem 1.25rem;
    background: white;
    padding: 1.25rem;
    border-radius: 8px;
    border: 1px solid #e2e8f0;
}

.list-field label {
    font-weight: 700;
    color: #1e293b;
    margin-bottom: 1rem;
    display: block;
    font-size: 0.775rem;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}

.list-items {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    margin-bottom: 1rem;
}

.list-item {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    padding: 0.5rem;
    background: #f8fafc;
    border-radius: 6px;
    border: 1px solid #e2e8f0;
}

.list-item input {
    flex: 1;
    padding: 0.45rem;
    border: 2px solid #e5e7eb;
    border-radius: 6px;
    font-size: 0.875rem;
    background: white;
    transition: all 0.2s ease;
}

.list-item input:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.remove-item-button {
    padding: 0.5rem;
    background: #ef4444;
    color: white;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    font-weight: bold;
    font-size: 1rem;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s ease;
}

.remove-item-button:hover {
    background: #dc2626;
    transform: scale(1.1);
}

.add-item-button {
    padding: 0.75rem 1.25rem;
    background: linear-gradient(135deg, #10b981, #059669);
    color: white;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    font-weight: 600;
    font-size: 0.875rem;
    transition: all 0.2s ease;
    box-shadow: 0 1px 3px rgba(16, 185, 129, 0.3);
    margin-top: 0.5rem;
}

.add-item-button:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(16, 185, 129, 0.4);
}

.loading-message {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    padding: 3rem;
    color: #6b7280;
}

.loading-spinner {
    width: 20px;
    height: 20px;
    border: 2px solid #f3f3f3;
    border-top: 2px solid #667eea;
    border-radius: 50%;
    animation: spin 1s linear infinite;
}

.error-message {
    background: #fee2e2;
    border: 1px solid #fecaca;
    color: #dc2626;
    padding: 1rem;
    border-radius: 10px;
    text-align: center;
    margin: 1rem;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    flex-direction: column;
}

.error-message .error-details {
    margin: 0;
    font-family: inherit;
    white-space: pre-wrap;
    text-align: left;
    background: rgba(0, 0, 0, 0.05);
    padding: 0.75rem;
    border-radius: 4px;
    font-size: 0.875rem;
    line-height: 1.5;
    max-width: 100%;
    overflow-x: auto;
}

.retry-button,
.load-button {
    padding: 0.5rem 1rem;
    background: #667eea;
    color: white;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    font-weight: 600;
}

.empty-state {
    display: flex;
    justify-content: center;
    align-items: center;
    padding: 3rem;
}

@keyframes spin {
    0% {
        transform: rotate(0deg);
    }
    100% {
        transform: rotate(360deg);
    }
}

/* Responsive design */
@media (max-width: 768px) {
    .config-header {
        flex-direction: column;
        gap: 1rem;
        text-align: center;
    }

    .config-actions {
        flex-wrap: wrap;
        justify-content: center;
    }

    .form-grid {
        grid-template-columns: 1fr;
    }

    .form-field.checkbox-grid {
        grid-template-columns: 1fr;
    }
}

/* Two column layout for hostnames and index files */
.two-column-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
    margin-bottom: 1rem;
}

/* Three column layout for lists */
.three-column-layout {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 1.5rem;
    margin-bottom: 1rem;
}

.third-width {
    min-width: 200px;
}

/* TLS Settings Section */
.subsection-title {
    margin: 0 0 1rem 0;
    font-size: 0.875rem;
    font-weight: 700;
    color: #1e293b;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}

.tls-grid {
    padding: 0;
    background: transparent;
    max-width: none;
    grid-template-columns: 1fr 1fr;
}

.tls-grid .form-field:nth-child(3),
.tls-grid .form-field:nth-child(4) {
    grid-column: span 2;
}

.tls-grid-full {
    display: flex;
    flex-direction: column;
    gap: 1rem;
}

.tls-paths-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
}

.tls-content-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
}

@media (max-width: 1024px) {
    .tls-grid .form-field:nth-child(3),
    .tls-grid .form-field:nth-child(4) {
        grid-column: span 1;
    }

    .tls-paths-row {
        grid-template-columns: 1fr;
    }

    .tls-content-row {
        grid-template-columns: 1fr;
    }
}

.tls-content-textarea {
    padding: 0.75rem;
    border: 2px solid #e5e7eb;
    border-radius: 8px;
    font-size: 0.875rem;
    font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace;
    line-height: 1.4;
    resize: vertical;
    min-height: 120px;
    background: white;
    transition: all 0.2s ease;
}

.tls-content-textarea:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.tls-content-textarea::placeholder {
    color: #9ca3af;
    font-size: 0.8rem;
}

/* Request Processing Section */
.request-processing-section,
.processors-section,
.tls-settings-section {
    margin: 1rem 1.25rem;
    padding: 5px;
    background: #f8fafc;
    border-radius: 8px;
    border: 1px solid #e2e8f0;
}

.processors-toolbar {
    display: flex;
    gap: 0.5rem;
    margin: 0 0 0 20px;
    flex-wrap: wrap;
}

.processor-item {
    background: white;
    border-radius: 8px;
    border: 1px solid #e2e8f0;
    margin: 0.75rem 1rem;
}

.processor-item .item-header {
    background: linear-gradient(135deg, #fefcfb 0%, #f3f4f6 100%);
    border-left: 3px solid #8b5cf6;
    margin-left: -3px;
}

.processor-item .item-header h6 {
    margin: 0;
    font-weight: 700;
    color: #6b21a8;
    font-size: 0.9rem;
}

.processor-config {
    padding: 0;
}

.info-field {
    margin: 1rem 0rem;
    padding: 0.75rem;
    background: #f0f9ff;
    border: 1px solid #bae6fd;
    border-radius: 6px;
    color: #0c4a6e;
    font-size: 0.875rem;
}

.info-field p {
    margin: 0;
}

.info-field strong {
    color: #0369a1;
}

/* Binding associations list styling */
.binding-associations-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
}

/* Associated bindings section */
.associated-bindings-section {
    margin: 1rem 1.25rem;
    padding: 1.25rem;
    background: #f0f9ff;
    border-radius: 8px;
    border: 1px solid #bae6fd;
}

.binding-associations-flex {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 0.75rem;
}

.checkbox-association-flex {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    background: white;
    border-radius: 6px;
    border: 1px solid #e0f2fe;
    cursor: pointer;
    transition: all 0.2s ease;
    white-space: nowrap;
}

.checkbox-association-flex:hover {
    background: #f8fafc;
    border-color: #0ea5e9;
    box-shadow: 0 2px 4px rgba(14, 165, 233, 0.1);
}

.checkbox-association-flex input[type='checkbox'] {
    margin: 0;
    width: 16px;
    height: 16px;
    accent-color: #0ea5e9;
}

/* Admin portal specific layout */
.admin-portal-layout {
    display: grid;
    grid-template-columns: 140px 140px 1fr 1fr;
    grid-template-areas:
        'checkbox checkbox checkbox checkbox'
        'ip port webroot indexfile';
    gap: 1rem;
    align-items: end;
}

.admin-portal-layout .form-field:nth-child(1) {
    grid-area: checkbox;
    align-self: start;
    margin-bottom: 0.5rem;
}

.admin-portal-layout .form-field:nth-child(2) {
    grid-area: ip;
}

.admin-portal-layout .form-field:nth-child(3) {
    grid-area: port;
}

.admin-portal-layout .form-field:nth-child(4) {
    grid-area: webroot;
}

.admin-portal-layout .form-field:nth-child(5) {
    grid-area: indexfile;
}

/* Subsection styles */
.subsection {
    background: #f8fafc;
    border: 1px solid #e2e8f0;
    border-radius: 8px;
    padding: 1.5rem;
    margin: 1rem 0;
}

.subsection-header {
    display: flex;
    align-items: center;
    margin-bottom: 1rem;
    padding-bottom: 0.75rem;
    border-bottom: 1px solid #e2e8f0;
}

.subsection-header h4 {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 600;
    color: #374151;
}

/* Array field styles */
.array-field {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
}

.array-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
}

.array-item input {
    flex: 1;
}

/* Binding associations styles */
.checkbox-grid-associations {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
    gap: 0.75rem;
    margin-top: 0.5rem;
}

.checkbox-association {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    background: #f8fafc;
    border-radius: 6px;
    border: 1px solid #e2e8f0;
    cursor: pointer;
    transition: all 0.2s ease;
}

.checkbox-association:hover {
    background: #f1f5f9;
    border-color: #cbd5e1;
}

.checkbox-association input[type='checkbox'] {
    margin: 0;
    width: 16px;
    height: 16px;
    accent-color: #667eea;
}

.empty-association-warning {
    margin-top: 1rem;
    padding: 0.75rem;
    background: #fef3c7;
    border: 1px solid #f59e0b;
    border-radius: 6px;
    color: #92400e;
    font-size: 0.875rem;
    font-weight: 500;
}

.empty-association-warning-inline {
    grid-column: 1 / -1;
    margin-top: 0.5rem;
    padding: 0.5rem 0.75rem;
    background: #fef3c7;
    border: 1px solid #f59e0b;
    border-radius: 6px;
    color: #92400e;
    font-size: 0.875rem;
    font-weight: 500;
}

@media (max-width: 1024px) {
    .three-column-layout {
        grid-template-columns: 1fr 1fr;
    }
}

@media (max-width: 768px) {
    .two-column-layout {
        grid-template-columns: 1fr;
    }

    .three-column-layout {
        grid-template-columns: 1fr;
    }

    .tls-grid {
        grid-template-columns: 1fr;
    }

    .admin-portal-layout {
        grid-template-columns: 1fr;
        grid-template-areas:
            'checkbox'
            'ip'
            'port';
    }

    .checkbox-grid-associations {
        grid-template-columns: 1fr;
    }

    .binding-associations-flex {
        flex-direction: column;
    }

    .checkbox-association-flex {
        justify-content: flex-start;
    }
}

/* Top Actions */
.top-actions {
    margin-bottom: 0;
    padding-bottom: 0;
    border-bottom: 1px solid #f1f5f9;
}

/* Bottom Actions */
.bottom-actions {
    margin-top: 2rem;
    padding-top: 1.5rem;
    border-top: 2px solid #e5e7eb;
}

.config-editor-inline .bottom-actions {
    border-top: 1px solid #f1f5f9;
    margin-top: 1.5rem;
    padding-top: 1rem;
}

.top-buttons,
.bottom-buttons {
    display: flex;
    gap: 0.75rem;
    align-items: center;
    margin-bottom: 1rem;
}

.save-button.top,
.save-button.bottom,
.reset-button.top,
.reset-button.bottom {
    padding: 0.75rem 1.5rem;
    font-size: 0.875rem;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    border: none;
}

.save-button.top,
.save-button.bottom {
    background: linear-gradient(135deg, #059669, #047857);
    color: white;
    box-shadow: 0 1px 3px rgba(16, 185, 129, 0.2);
}

.save-button.top:not(:disabled):hover,
.save-button.bottom:not(:disabled):hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(16, 185, 129, 0.2);
}

.reset-button.top,
.reset-button.bottom {
    background: #d97706;
    color: white;
    box-shadow: 0 1px 3px rgba(245, 158, 11, 0.2);
}

.reset-button.top:hover,
.reset-button.bottom:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 8px rgba(245, 158, 11, 0.2);
}

.save-button.top:disabled,
.save-button.bottom:disabled,
.reset-button.top:disabled,
.reset-button.bottom:disabled {
    background: #9ca3af;
    cursor: not-allowed;
    box-shadow: none;
}

.save-button.top:disabled:hover,
.save-button.bottom:disabled:hover,
.reset-button.top:disabled:hover,
.reset-button.bottom:disabled:hover {
    transform: none;
    box-shadow: none;
}

.changes-indicator-top,
.changes-indicator-bottom {
    display: inline-block;
    background: #fffbeb;
    border: 1px solid #f59e0b;
    color: #92400e;
    padding: 0.5rem 1rem;
    border-radius: 6px;
    font-size: 0.875rem;
    font-weight: 600;
    width: auto;
}

@media (max-width: 768px) {
    .top-buttons,
    .bottom-buttons {
        flex-direction: column;
        align-items: stretch;
    }

    .save-button.top,
    .save-button.bottom,
    .reset-button.top,
    .reset-button.bottom {
        width: 100%;
    }
}

/* Request Handler Specific Styles */
.handler-type-badge {
    font-size: 0.65rem;
    font-weight: 700;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    background: #f3e8ff;
    color: #7c3aed;
}

.priority-badge {
    font-size: 0.65rem;
    font-weight: 600;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    background: #fef3c7;
    color: #92400e;
}

.priority-controls {
    display: inline-flex;
    gap: 0.25rem;
    align-items: center;
}

.priority-adjust-button {
    padding: 0.1rem 0.35rem;
    font-size: 0.7rem;
    line-height: 1;
    border-radius: 4px;
    border: 1px solid #f59e0b;
    background: #fef3c7;
    color: #92400e;
    cursor: pointer;
}

.priority-adjust-button:hover {
    background: #fde68a;
}

.handler-select {
    flex: 1;
    padding: 0.45rem;
    border: 2px solid #e5e7eb;
    border-radius: 6px;
    font-size: 0.875rem;
    background: white;
    transition: all 0.2s ease;
}

.handler-select:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.rewrite-select {
    flex: 1;
    padding: 0.45rem;
    border: 2px solid #e5e7eb;
    border-radius: 6px;
    font-size: 0.875rem;
    background: white;
    transition: all 0.2s ease;
}

.rewrite-select:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.doc-link {
    margin-bottom: 0.75rem;
    font-size: 0.8rem;
}

.doc-link a {
    color: #667eea;
    text-decoration: none;
    font-weight: 500;
    transition: all 0.2s ease;
}

.doc-link a:hover {
    color: #764ba2;
    text-decoration: underline;
}

.form-field select {
    padding: 0.375rem;
    border: 2px solid #e5e7eb;
    border-radius: 8px;
    font-size: 0.875rem;
    background: white;
    transition: all 0.2s ease;
}

.form-field select:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.list-item.key-value {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 0.5rem;
    align-items: center;
}

.list-item.url-rewrite-item {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 0.5rem;
}

.rewrite-row {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 0.75rem;
    align-items: end;
}

.rewrite-field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex: 1;
    min-width: 0;
}

.rewrite-remove-button {
    margin-bottom: 0.15rem;
}

.inline-checkbox {
    gap: 0.5rem;
    margin: 0;
    display: flex !important;
    align-self: flex-start;
    margin: 0.5rem 0;
}

.inline-checkbox input[type='checkbox'] {
    margin: 0;
}

@media (max-width: 640px) {
    .rewrite-row {
        grid-template-columns: 1fr;
    }

    .rewrite-remove-button {
        justify-self: end;
        margin-bottom: 0;
    }
}

.rewrite-label {
    font-size: 0.75rem;
    font-weight: 600;
    color: #4b5563;
    margin: 0;
    padding: 0;
}

.key-input,
.value-input {
    padding: 0.45rem;
    border: 2px solid #e5e7eb;
    border-radius: 6px;
    font-size: 0.875rem;
    background: white;
    transition: all 0.2s ease;
}

.key-input:focus,
.value-input:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.handler-info {
    margin-top: 0.75rem;
    padding: 0.75rem;
    background: #f0f9ff;
    border: 1px solid #bae6fd;
    border-radius: 6px;
    color: #0c4a6e;
    font-size: 0.875rem;
}

.handler-info p {
    margin: 0;
}

.handler-info strong {
    color: #0369a1;
}

/* Request Handler Two Column Layout */
.handler-two-column-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2rem;
    padding: 1rem 1.25rem;
    background: white;
}

.handler-column {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
}

.handler-column .form-field {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
}

.handler-column .form-field label {
    font-weight: 600;
    color: #374151;
    font-size: 0.875rem;
    margin-bottom: 0.25rem;
}

.handler-column .form-field input,
.handler-column .form-field select {
    padding: 0.375rem;
    border: 2px solid #e5e7eb;
    border-radius: 8px;
    font-size: 0.875rem;
    transition: all 0.2s ease;
    background: white;
}

.handler-column .form-field input:focus,
.handler-column .form-field select:focus {
    outline: none;
    border-color: #667eea;
    box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

@media (max-width: 1024px) {
    .handler-two-column-layout {
        grid-template-columns: 1fr;
        gap: 1rem;
    }
}

/* Modal Styles */
.modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    animation: modalFadeIn 0.2s ease;
}

.modal-content {
    background: white;
    border-radius: 12px;
    max-width: 500px;
    width: 90%;
    max-height: 80vh;
    overflow-y: auto;
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.15);
    border: 1px solid #e5e7eb;
    animation: modalSlideIn 0.3s ease;
}

.modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1.5rem;
    border-bottom: 1px solid #f1f5f9;
    background: linear-gradient(135deg, #fef3c7 0%, #fed7aa 100%);
    border-radius: 12px 12px 0 0;
}

.modal-header h3 {
    margin: 0;
    color: #92400e;
    font-size: 1.25rem;
    font-weight: 700;
    display: flex;
    align-items: center;
    gap: 0.5rem;
}

.modal-close-button {
    background: none;
    border: none;
    font-size: 1.5rem;
    color: #6b7280;
    cursor: pointer;
    padding: 0.25rem;
    border-radius: 4px;
    transition: all 0.2s ease;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
}

.modal-close-button:hover {
    background: rgba(0, 0, 0, 0.1);
    color: #374151;
}

.modal-body {
    padding: 1.5rem;
}

.modal-warning {
    display: flex;
    gap: 1rem;
    align-items: flex-start;
}

.modal-error {
    display: flex;
    gap: 1rem;
    align-items: flex-start;
}

.warning-icon {
    font-size: 1.5rem;
    flex-shrink: 0;
    margin-top: 0.25rem;
}

.error-icon {
    font-size: 1.5rem;
    flex-shrink: 0;
    margin-top: 0.25rem;
}

.modal-warning strong,
.modal-error strong {
    color: #1f2937;
    font-size: 1.1rem;
    margin-bottom: 0.5rem;
    display: block;
}

.modal-warning p,
.modal-error p {
    color: #4b5563;
    margin: 0.5rem 0;
    line-height: 1.5;
}

.modal-warning ul {
    color: #4b5563;
    margin: 0.5rem 0;
    padding-left: 1.25rem;
    line-height: 1.6;
}

.modal-warning li {
    margin-bottom: 0.25rem;
}

.warning-note {
    background: #fef3c7;
    border: 1px solid #f59e0b;
    border-radius: 6px;
    padding: 0.75rem;
    margin-top: 1rem;
    color: #92400e !important;
    font-weight: 500;
    font-size: 0.875rem;
}

.modal-footer {
    padding: 1rem 1.5rem;
    border-top: 1px solid #f1f5f9;
    background: #f8fafc;
    border-radius: 0 0 12px 12px;
}

.modal-actions,
.modal-actions-error {
    display: flex;
    gap: 0.75rem;
    justify-content: flex-end;
}

.modal-button {
    padding: 0.75rem 1.5rem;
    border: none;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
    font-size: 0.875rem;
}

.modal-button.secondary {
    background: #f3f4f6;
    color: #374151;
    border: 1px solid #d1d5db;
}

.modal-button.secondary:hover {
    background: #e5e7eb;
    transform: translateY(-1px);
}

.modal-button.danger {
    background: linear-gradient(135deg, #ef4444, #dc2626);
    color: white;
    box-shadow: 0 2px 4px rgba(239, 68, 68, 0.2);
}

.modal-button.danger:not(:disabled):hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(239, 68, 68, 0.3);
}

.modal-button:disabled {
    background: #9ca3af;
    color: #6b7280;
    cursor: not-allowed;
    box-shadow: none;
}

.modal-button:disabled:hover {
    transform: none;
    box-shadow: none;
}

@keyframes modalFadeIn {
    from {
        opacity: 0;
    }
    to {
        opacity: 1;
    }
}

@keyframes modalSlideIn {
    from {
        opacity: 0;
        transform: translateY(-20px) scale(0.95);
    }
    to {
        opacity: 1;
        transform: translateY(0) scale(1);
    }
}

@media (max-width: 768px) {
    .modal-content {
        width: 95%;
        margin: 1rem;
    }

    .modal-actions,
    .modal-actions-error {
        flex-direction: column;
    }

    .modal-button {
        width: 100%;
    }
}
</style>
