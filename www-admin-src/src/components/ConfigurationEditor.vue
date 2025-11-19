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

// State
const isLoading = ref(false);
const isSaving = ref(false);
const error = ref('');
const saveError = ref('');
const successMessage = ref('');
const originalConfig = ref(null);
const config = ref(null);

// Track which sections are expanded (all collapsed by default)
const expandedSections = reactive({
    bindings: false,
    sites: false,
    requestHandlers: false,
    core: false,
});

// Track which individual items are expanded
const expandedItems = reactive({
    bindings: {},
    sites: {},
    requestHandlers: {},
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

// Save configuration
const saveConfiguration = async () => {
    isSaving.value = true;
    saveError.value = '';
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

        const responseData = await response.json();

        if (response.ok) {
            originalConfig.value = JSON.parse(JSON.stringify(config.value)); // Update original
            successMessage.value = responseData.message || 'Configuration saved successfully!';
            saveError.value = ''; // Clear any previous save errors
            setTimeout(() => {
                successMessage.value = '';
            }, 10000); // Show for 10 seconds since restart might be required
        } else {
            // Handle different types of errors - DON'T reset config, keep user's changes
            if (response.status === 400) {
                // Validation error
                if (responseData.details && typeof responseData.details === 'string') {
                    // Single error message
                    saveError.value = `${responseData.details}`;
                } else if (responseData.details && Array.isArray(responseData.details)) {
                    // Multiple validation errors - format as bullet list
                    saveError.value = `Configuration validation failed:\n• ${responseData.details
                        .split(';')
                        .map((err) => err.trim())
                        .join('\n• ')}`;
                } else {
                    saveError.value = responseData.error || 'Configuration validation failed';
                }
            } else if (response.status === 401) {
                saveError.value = 'Authentication required. Please log in again.';
            } else {
                saveError.value = responseData.error || 'Failed to save configuration';
            }
            successMessage.value = ''; // Clear success message if there's an error
        }
    } catch (err) {
        console.error('Config saving error:', err);
        successMessage.value = ''; // Clear success message if there's an error
        if (err.name === 'TypeError' && err.message.includes('Failed to fetch')) {
            saveError.value = 'Network error: Unable to connect to server';
        } else {
            saveError.value = 'Network error while saving configuration';
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

const toggleRequestHandler = (handlerIndex) => {
    if (!expandedItems.requestHandlers[handlerIndex]) {
        expandedItems.requestHandlers[handlerIndex] = false;
    }
    expandedItems.requestHandlers[handlerIndex] = !expandedItems.requestHandlers[handlerIndex];
};

// Helper functions to check if items are expanded
const isBindingExpanded = (bindingIndex) => {
    return expandedItems.bindings[bindingIndex] || false;
};

const isSiteExpanded = (siteIndex) => {
    return expandedItems.sites[siteIndex] || false;
};

const isRequestHandlerExpanded = (handlerIndex) => {
    return expandedItems.requestHandlers[handlerIndex] || false;
};

// Toggle core subsections
const toggleCoreSubsection = (subsection) => {
    expandedItems.coreSubsections[subsection] = !expandedItems.coreSubsections[subsection];
};

const isCoreSubsectionExpanded = (subsection) => {
    return expandedItems.coreSubsections[subsection] || false;
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
        web_root: './www-default/',
        web_root_index_file_list: ['index.html'],
        enabled_handlers: [],
        tls_cert_path: '',
        tls_cert_content: '',
        tls_key_path: '',
        tls_key_content: '',
        rewrite_functions: [],
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

// Add new request handler
const addRequestHandler = () => {
    if (!config.value.request_handlers) {
        config.value.request_handlers = [];
    }

    // Generate a unique ID
    const existingIds = config.value.request_handlers.map((h) => parseInt(h.id)).filter((id) => !isNaN(id));
    const newId = existingIds.length > 0 ? (Math.max(...existingIds) + 1).toString() : '1';

    config.value.request_handlers.push({
        id: newId,
        is_enabled: true,
        name: 'New Handler',
        handler_type: 'php',
        request_timeout: 30,
        concurrent_threads: 0,
        file_match: ['.php'],
        executable: '',
        ip_and_port: '',
        other_webroot: '',
        extra_handler_config: [],
        extra_environment: [],
    });
};

// Remove request handler
const removeRequestHandler = (index) => {
    if (config.value.request_handlers && config.value.request_handlers.length > index) {
        const handlerId = config.value.request_handlers[index].id;
        config.value.request_handlers.splice(index, 1);

        // Remove handler ID from all sites that reference it
        if (config.value.sites) {
            config.value.sites.forEach((site) => {
                if (site.enabled_handlers) {
                    site.enabled_handlers = site.enabled_handlers.filter((id) => id !== handlerId);
                }
            });
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

// Add index file to site
const addIndexFile = (siteIndex) => {
    if (config.value.sites && config.value.sites[siteIndex]) {
        config.value.sites[siteIndex].web_root_index_file_list.push('index.html');
    }
};

// Remove index file from site
const removeIndexFile = (siteIndex, fileIndex) => {
    if (config.value.sites && config.value.sites[siteIndex] && config.value.sites[siteIndex].web_root_index_file_list.length > fileIndex) {
        config.value.sites[siteIndex].web_root_index_file_list.splice(fileIndex, 1);
    }
};

// Add enabled handler to site
const addEnabledHandler = (siteIndex) => {
    if (config.value.sites && config.value.sites[siteIndex]) {
        // Add the first available handler ID, or empty string if none available
        const availableHandlers = getAvailableRequestHandlers();
        const handlerId = availableHandlers.length > 0 ? availableHandlers[0].id : '';
        config.value.sites[siteIndex].enabled_handlers.push(handlerId);
    }
};

// Remove enabled handler from site
const removeEnabledHandler = (siteIndex, handlerIndex) => {
    if (config.value.sites && config.value.sites[siteIndex] && config.value.sites[siteIndex].enabled_handlers.length > handlerIndex) {
        config.value.sites[siteIndex].enabled_handlers.splice(handlerIndex, 1);
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

// Request handler helper functions
const addFileMatch = (handlerIndex) => {
    if (config.value.request_handlers && config.value.request_handlers[handlerIndex]) {
        config.value.request_handlers[handlerIndex].file_match.push('.html');
    }
};

const removeFileMatch = (handlerIndex, matchIndex) => {
    if (config.value.request_handlers && config.value.request_handlers[handlerIndex] && config.value.request_handlers[handlerIndex].file_match.length > matchIndex) {
        config.value.request_handlers[handlerIndex].file_match.splice(matchIndex, 1);
    }
};

const addHandlerConfig = (handlerIndex) => {
    if (config.value.request_handlers && config.value.request_handlers[handlerIndex]) {
        config.value.request_handlers[handlerIndex].extra_handler_config.push(['key', 'value']);
    }
};

const removeHandlerConfig = (handlerIndex, configIndex) => {
    if (config.value.request_handlers && config.value.request_handlers[handlerIndex] && config.value.request_handlers[handlerIndex].extra_handler_config.length > configIndex) {
        config.value.request_handlers[handlerIndex].extra_handler_config.splice(configIndex, 1);
    }
};

const addEnvironmentVar = (handlerIndex) => {
    if (config.value.request_handlers && config.value.request_handlers[handlerIndex]) {
        config.value.request_handlers[handlerIndex].extra_environment.push(['ENV_VAR', 'value']);
    }
};

const removeEnvironmentVar = (handlerIndex, envIndex) => {
    if (config.value.request_handlers && config.value.request_handlers[handlerIndex] && config.value.request_handlers[handlerIndex].extra_environment.length > envIndex) {
        config.value.request_handlers[handlerIndex].extra_environment.splice(envIndex, 1);
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
            <div v-if="saveError" class="save-error-message">
                <div class="error-header">
                    <span class="error-icon">⚠️</span>
                    <strong>Configuration Save Failed</strong>
                </div>
                <pre v-if="saveError.includes('\\n')" class="error-details">{{ saveError }}</pre>
                <span v-else>{{ saveError }}</span>
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
                                <div class="form-field small-field">
                                    <label>IP Address</label>
                                    <input v-model="binding.ip" type="text" />
                                </div>
                                <div class="form-field small-field">
                                    <label>Port</label>
                                    <input v-model.number="binding.port" type="number" min="1" max="65535" />
                                </div>
                                <div class="form-field checkbox-grid">
                                    <label>
                                        <input v-model="binding.is_admin" type="checkbox" />
                                        Admin portal
                                    </label>
                                    <label>
                                        <input v-model="binding.is_tls" type="checkbox" />
                                        Enable TLS (https://)
                                    </label>
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
                                <span v-if="getSiteBindings(site.id).some(bindingId => config.bindings?.find(b => b.id === bindingId)?.is_admin)" class="admin-badge">ADMIN PORTAL</span>
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

                            <div class="form-grid compact">
                                <div class="form-field">
                                    <div v-if="getSiteBindings(site.id).length === 0" class="empty-association-warning-inline">⚠️ This site is not associated with any bindings and will not be accessible.</div>
                                </div>
                            </div>

                            <div class="form-grid compact">
                                <div class="form-field">
                                    <label>Web Root

                                        <span class="help-icon" data-tooltip="The file root directory for the website's files. This is where the server will look for the site's content. Can be absolute full path or a relative path to Grux server location, such as './www-default'.">?</span>
                                    </label>
                                    <input v-model="site.web_root" type="text" />
                                </div>
                            </div>

                            <div class="form-grid compact">
                                <div v-if="site.access_log_enabled" class="form-field">
                                    <label>
                                        Access Log File
                                        <span class="help-icon" data-tooltip="Path to the access log file. If relative to grux base directory, use it like this './logs/mylog.log'. You can also have a full absolute path like '/var/logs/mylog.log' or 'C:/logs/mylog.log'.">?</span>
                                    </label>
                                    <input v-model="site.access_log_file" type="text" placeholder="Path to log file" />
                                </div>
                            </div>

                            <!-- TLS Certificate Settings -->
                            <div class="tls-settings-section">
                                <h5 class="subsection-title">TLS Certificate Settings (Optional)</h5>
                                <div class="info-field">
                                    <p><strong>Note:</strong> You can either specify file paths or paste the certificate/key content directly. If both are provided, the file paths take precedence.</p>
                                </div>
                                <div class="tls-grid-full">
                                    <div class="tls-paths-row">
                                        <div class="form-field">
                                            <label>Certificate Path
                                                <span class="help-icon" data-tooltip="Path to the TLS certificate file. You can specify an absolute path or a path relative to the Grux server base directory. Such as './certs/mycert.pem' or '/etc/ssl/certs/mycert.pem'.">?</span>
                                            </label>
                                            <input v-model="site.tls_cert_path" type="text" placeholder="Path to certificate file" />
                                        </div>
                                        <div class="form-field">
                                            <label>Private Key Path
                                                <span class="help-icon" data-tooltip="Path to the TLS private key file. You can specify an absolute path or a path relative to the Grux server base directory. Such as './certs/mykey.pem' or '/etc/ssl/private/mykey.pem'.">?</span>
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

                            <!-- Lists in two columns layout -->
                            <div class="two-column-layout">
                                <!-- Hostnames -->
                                <div class="list-field compact third-width">
                                    <label>Hostnames (use * to match all hostnames)</label>
                                    <div class="list-items">
                                        <div v-for="(hostname, hostnameIndex) in site.hostnames" :key="hostnameIndex" class="list-item">
                                            <input v-model="site.hostnames[hostnameIndex]" type="text" />
                                            <button @click="removeHostname(siteIndex, hostnameIndex)" class="remove-item-button">×</button>
                                        </div>
                                        <button @click="addHostname(siteIndex)" class="add-item-button">+ Add Hostname</button>
                                    </div>
                                </div>

                                <!-- Index Files -->
                                <div class="list-field compact third-width">
                                    <label>Index Files</label>
                                    <div class="list-items">
                                        <div v-for="(file, fileIndex) in site.web_root_index_file_list" :key="fileIndex" class="list-item">
                                            <input v-model="site.web_root_index_file_list[fileIndex]" type="text" />
                                            <button @click="removeIndexFile(siteIndex, fileIndex)" class="remove-item-button">×</button>
                                        </div>
                                        <button @click="addIndexFile(siteIndex)" class="add-item-button">+ Add Index File</button>
                                    </div>
                                </div>
                            </div>

                            <!-- Second row for handlers and rewrite functions -->
                            <div class="two-column-layout">
                                <!-- Enabled Handlers -->
                                <div class="list-field compact half-width">
                                    <label>Enabled Request Handlers</label>
                                    <div class="list-items">
                                        <div v-for="(handlerId, handlerIndex) in site.enabled_handlers" :key="handlerIndex" class="list-item">
                                            <select v-model="site.enabled_handlers[handlerIndex]" class="handler-select">
                                                <option value="">-- Select Handler --</option>
                                                <option v-for="handler in getAvailableRequestHandlers()" :key="handler.id" :value="handler.id">
                                                    {{ handler.label }}
                                                </option>
                                            </select>
                                            <button @click="removeEnabledHandler(siteIndex, handlerIndex)" class="remove-item-button">×</button>
                                        </div>
                                        <button @click="addEnabledHandler(siteIndex)" class="add-item-button">+ Add Handler</button>
                                    </div>
                                    <div v-if="site.enabled_handlers.length === 0" class="handler-info">
                                        <p><strong>Info:</strong> No request handlers are enabled for this site. Static files will be served directly.</p>
                                    </div>
                                </div>

                                <!-- Rewrite Functions -->
                                <div class="list-field compact half-width">
                                    <label>Rewrite Functions</label>
                                    <div class="list-items">
                                        <div v-for="(func, funcIndex) in site.rewrite_functions" :key="funcIndex" class="list-item">
                                            <input v-model="site.rewrite_functions[funcIndex]" type="text" placeholder="Function name" />
                                            <button @click="removeRewriteFunction(siteIndex, funcIndex)" class="remove-item-button">×</button>
                                        </div>
                                        <button @click="addRewriteFunction(siteIndex)" class="add-item-button">+ Add Function</button>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <!-- Request Handlers Section -->
            <div class="config-section">
                <div class="section-header" @click="toggleSection('requestHandlers')">
                    <span class="section-icon" :class="{ expanded: expandedSections.requestHandlers }">▶</span>
                    <span class="section-title-icon">⚙️</span>
                    <h3>Request Handlers</h3>
                    <button @click.stop="addRequestHandler" class="add-button">+ Add Handler</button>
                </div>

                <div v-if="expandedSections.requestHandlers" class="section-content">
                    <div v-if="!config.request_handlers || config.request_handlers.length === 0" class="empty-state-section">
                        <div class="empty-icon">⚙️</div>
                        <p>No request handlers configured</p>
                        <button @click="addRequestHandler" class="add-button">+ Add First Handler</button>
                    </div>

                    <!-- Request Handlers List -->
                    <div v-for="(handler, handlerIndex) in config.request_handlers" :key="handler.id" class="server-item">
                        <div class="item-header compact" @click="toggleRequestHandler(handlerIndex)">
                            <div class="header-left">
                                <span class="section-icon" :class="{ expanded: isRequestHandlerExpanded(handlerIndex) }">▶</span>
                                <span class="hierarchy-indicator handler-indicator">⚙️</span>
                                <h4>{{ handler.name }}</h4>
                                <span class="handler-type-badge">{{ handler.handler_type.toUpperCase() }}</span>
                                <span v-if="!handler.is_enabled" class="admin-badge">DISABLED</span>
                            </div>
                            <button @click.stop="removeRequestHandler(handlerIndex)" class="remove-button compact">Remove</button>
                        </div>

                        <!-- Request Handler Content -->
                        <div v-if="isRequestHandlerExpanded(handlerIndex)" class="item-content">
                            <!-- Enabled checkbox at the top -->
                            <div class="form-grid compact">
                                <div class="form-field checkbox-grid compact">
                                    <label>
                                        <input v-model="handler.is_enabled" type="checkbox" />
                                        Enabled
                                    </label>
                                </div>
                            </div>

                            <!-- Two column layout for main fields -->
                            <div class="handler-two-column-layout">
                                <!-- First column -->
                                <div class="handler-column">
                                    <div class="form-field">
                                        <label>Handler Name</label>
                                        <input v-model="handler.name" type="text" placeholder="e.g., PHP Handler" />
                                    </div>
                                    <div class="form-field">
                                        <label>Handler Type</label>
                                        <select v-model="handler.handler_type">
                                            <option value=""></option>
                                            <option value="php">PHP</option>
                                        </select>
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

                                <!-- Second column -->
                                <div class="handler-column">
                                    <div class="form-field">
                                        <label>Executable Path</label>
                                        <input v-model="handler.executable" type="text" placeholder="Path to executable (e.g., php-cgi.exe)" />
                                    </div>
                                    <div class="form-field">
                                        <label>IP and Port (optional, for FastCGI)</label>
                                        <input v-model="handler.ip_and_port" type="text" placeholder="e.g., 127.0.0.1:9000" />
                                    </div>
                                    <div class="form-field">
                                        <label>Alternative Web Root (optional - Used in cases like PHP-FPM running in Docker, with its own file system)</label>
                                        <input v-model="handler.other_webroot" type="text" placeholder="Override site web root for this handler" />
                                    </div>
                                </div>
                            </div>

                            <!-- File Match Patterns -->
                            <div class="two-column-layout">
                                <div class="list-field compact half-width">
                                    <label>File Match Patterns</label>
                                    <div class="list-items">
                                        <div v-for="(pattern, patternIndex) in handler.file_match" :key="patternIndex" class="list-item">
                                            <input v-model="handler.file_match[patternIndex]" type="text" placeholder=".php" />
                                            <button @click="removeFileMatch(handlerIndex, patternIndex)" class="remove-item-button">×</button>
                                        </div>
                                        <button @click="addFileMatch(handlerIndex)" class="add-item-button">+ Add Pattern</button>
                                    </div>
                                </div>
                            </div>

                            <!-- Two column layout for config and environment -->
                            <div class="two-column-layout">
                                <!-- Extra Handler Config -->
                                <div class="list-field compact half-width">
                                    <label>Extra Handler Configuration</label>
                                    <div class="list-items">
                                        <div v-for="(configPair, configIndex) in handler.extra_handler_config" :key="configIndex" class="list-item key-value">
                                            <input v-model="handler.extra_handler_config[configIndex][0]" type="text" placeholder="Key" class="key-input" />
                                            <input v-model="handler.extra_handler_config[configIndex][1]" type="text" placeholder="Value" class="value-input" />
                                            <button @click="removeHandlerConfig(handlerIndex, configIndex)" class="remove-item-button">×</button>
                                        </div>
                                        <button @click="addHandlerConfig(handlerIndex)" class="add-item-button">+ Add Config</button>
                                    </div>
                                </div>

                                <!-- Extra Environment Variables -->
                                <div class="list-field compact half-width">
                                    <label>Extra Environment Variables</label>
                                    <div class="list-items">
                                        <div v-for="(envPair, envIndex) in handler.extra_environment" :key="envIndex" class="list-item key-value">
                                            <input v-model="handler.extra_environment[envIndex][0]" type="text" placeholder="ENV_VAR" class="key-input" />
                                            <input v-model="handler.extra_environment[envIndex][1]" type="text" placeholder="value" class="value-input" />
                                            <button @click="removeEnvironmentVar(handlerIndex, envIndex)" class="remove-item-button">×</button>
                                        </div>
                                        <button @click="addEnvironmentVar(handlerIndex)" class="add-item-button">+ Add Variable</button>
                                    </div>
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

        <!-- Empty State -->
        <div v-else class="empty-state">
            <button @click="loadConfiguration" class="load-button">Load Configuration</button>
        </div>
    </div>
</template>

<style scoped>
.config-editor {
    background: #ffffff;
    border-radius: 12px;
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.1);
    border: 1px solid #e5e7eb;
    overflow: hidden;
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

.list-field.compact {
    margin: 1rem;
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
    overflow: hidden;
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

.server-item {
    margin: 1rem;
    background: white;
    border-radius: 8px;
    border-left: 4px solid #3b82f6;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.05);
    overflow: hidden;
}

.binding-item {
    margin: 0.75rem 1rem;
    background: #f8fafc;
    border-radius: 8px;
    border-left: 3px solid #10b981;
    overflow: hidden;
}

.site-item {
    margin: 0.5rem 0.75rem;
    background: #ffffff;
    border-radius: 6px;
    border-left: 2px solid #f59e0b;
    overflow: hidden;
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
    justify-content: space-between;
    align-items: center;
    margin: 1rem 1.25rem 0.75rem 1.25rem;
    padding-bottom: 0.375rem;
    border-bottom: 2px solid #e2e8f0;
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
    padding: 0.375rem 0.75rem;
    font-size: 0.8rem;
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
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 1.5rem;
    padding: 1.5rem 1.25rem;
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
    gap: 1rem;
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
    gap: 0.75rem;
    font-size: 0.875rem;
    margin-bottom: 0.25rem;
}

.form-field input[type='text'],
.form-field input[type='number'] {
    padding: 0.875rem;
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
    gap: 1.5rem;
    margin-top: 1rem;
}

/* Three column layout for lists */
.three-column-layout {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 1.5rem;
    margin-top: 1rem;
}

.third-width {
    min-width: 200px;
}

/* TLS Settings Section */
.tls-settings-section {
    margin: 1rem 1.25rem;
    padding: 1.25rem;
    background: #f8fafc;
    border-radius: 8px;
    border: 1px solid #e2e8f0;
}

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

.form-field select {
    padding: 0.875rem;
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
    padding: 0.875rem;
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

/* Help Icon Styles */
.help-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: white;
    border-radius: 50%;
    font-size: 11px;
    font-weight: 700;
    cursor: help;
    margin-left: 6px;
    transition: all 0.3s ease;
    box-shadow: 0 2px 4px rgba(102, 126, 234, 0.2);
    position: relative;
}

.help-icon:hover {

    box-shadow: 0 4px 12px rgba(102, 126, 234, 0.3);
    background: linear-gradient(135deg, #764ba2 0%, #667eea 100%);
}

/* Enhanced tooltip styling */
.help-icon[data-tooltip]:hover::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: 125%;
    left: 50%;
    transform: translateX(-50%);
    background: #1f2937;
    color: white;
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
    z-index: 1000;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    opacity: 0;
    animation: tooltipFadeIn 0.2s ease forwards;
    max-width: 400px;
    min-width: 200px;
    white-space: normal;
    line-height: 1.4;
}

.help-icon[data-tooltip]:hover::before {
    content: '';
    position: absolute;
    bottom: 115%;
    left: 50%;
    transform: translateX(-50%);
    border: 6px solid transparent;
    border-top-color: #1f2937;
    z-index: 1000;
    opacity: 0;
    animation: tooltipFadeIn 0.2s ease forwards;
}

@keyframes tooltipFadeIn {
    from {
        opacity: 0;
        transform: translateX(-50%) translateY(-4px);
    }
    to {
        opacity: 1;
        transform: translateX(-50%) translateY(0);
    }
}

/* Alternative icon styles for different contexts */
.help-icon.info {
    background: linear-gradient(135deg, #06b6d4 0%, #0891b2 100%);
}

.help-icon.warning {
    background: linear-gradient(135deg, #f59e0b 0%, #d97706 100%);
}

.help-icon.success {
    background: linear-gradient(135deg, #10b981 0%, #059669 100%);
}
</style>
