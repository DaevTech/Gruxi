<script setup>
import { ref, reactive, computed, onMounted } from 'vue'

// Define props
const props = defineProps({
  user: {
    type: Object,
    required: true
  },
  inline: {
    type: Boolean,
    default: false
  }
})

// Define emits
const emit = defineEmits(['close'])

// State
const isLoading = ref(false)
const isSaving = ref(false)
const error = ref('')
const saveError = ref('')
const successMessage = ref('')
const originalConfig = ref(null)
const config = ref(null)

// Track which sections are expanded (all collapsed by default)
const expandedSections = reactive({
  servers: false,
  adminSite: false
})

// Track which individual items are expanded
const expandedItems = reactive({
  servers: {},
  bindings: {},
  sites: {}
})

// Check if config has unsaved changes
const hasUnsavedChanges = computed(() => {
  if (!originalConfig.value || !config.value) return false
  return JSON.stringify(originalConfig.value) !== JSON.stringify(config.value)
})

// Load configuration
const loadConfiguration = async () => {
  isLoading.value = true
  error.value = ''

  try {
    const response = await fetch('/config', {
      method: 'GET',
      headers: {
        'Authorization': `Bearer ${props.user.sessionToken}`,
        'Content-Type': 'application/json'
      }
    })

    if (response.ok) {
      const data = await response.json()
      originalConfig.value = JSON.parse(JSON.stringify(data)) // Deep copy
      config.value = data
    } else {
      error.value = 'Failed to load configuration'
    }
  } catch (err) {
    console.error('Config loading error:', err)
    error.value = 'Network error while loading configuration'
  } finally {
    isLoading.value = false
  }
}

// Save configuration
const saveConfiguration = async () => {
  isSaving.value = true
  saveError.value = ''
  successMessage.value = ''

  try {
    const response = await fetch('/config', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${props.user.sessionToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify(config.value)
    })

    const responseData = await response.json()

    if (response.ok) {
      originalConfig.value = JSON.parse(JSON.stringify(config.value)) // Update original
      successMessage.value = responseData.message || 'Configuration saved successfully!'
      saveError.value = '' // Clear any previous save errors
      setTimeout(() => {
        successMessage.value = ''
      }, 10000) // Show for 10 seconds since restart might be required
    } else {
      // Handle different types of errors - DON'T reset config, keep user's changes
      if (response.status === 400) {
        // Validation error
        if (responseData.details && typeof responseData.details === 'string') {
          // Single error message
          saveError.value = `${responseData.details}`
        } else if (responseData.details && Array.isArray(responseData.details)) {
          // Multiple validation errors - format as bullet list
          saveError.value = `Configuration validation failed:\n• ${responseData.details.split(';').map(err => err.trim()).join('\n• ')}`
        } else {
          saveError.value = responseData.error || 'Configuration validation failed'
        }
      } else if (response.status === 401) {
        saveError.value = 'Authentication required. Please log in again.'
      } else {
        saveError.value = responseData.error || 'Failed to save configuration'
      }
      successMessage.value = '' // Clear success message if there's an error
    }
  } catch (err) {
    console.error('Config saving error:', err)
    successMessage.value = '' // Clear success message if there's an error
    if (err.name === 'TypeError' && err.message.includes('Failed to fetch')) {
      saveError.value = 'Network error: Unable to connect to server'
    } else {
      saveError.value = 'Network error while saving configuration'
    }
  } finally {
    isSaving.value = false
  }
}

// Reset changes
const resetChanges = () => {
  if (originalConfig.value) {
    config.value = JSON.parse(JSON.stringify(originalConfig.value))
  }
}

// Toggle section expansion
const toggleSection = (section) => {
  expandedSections[section] = !expandedSections[section]
}

// Toggle individual item expansion
const toggleServer = (serverIndex) => {
  if (!expandedItems.servers[serverIndex]) {
    expandedItems.servers[serverIndex] = false
  }
  expandedItems.servers[serverIndex] = !expandedItems.servers[serverIndex]
}

const toggleBinding = (serverIndex, bindingIndex) => {
  const key = `${serverIndex}-${bindingIndex}`
  if (!expandedItems.bindings[key]) {
    expandedItems.bindings[key] = false
  }
  expandedItems.bindings[key] = !expandedItems.bindings[key]
}

const toggleSite = (serverIndex, bindingIndex, siteIndex) => {
  const key = `${serverIndex}-${bindingIndex}-${siteIndex}`
  if (!expandedItems.sites[key]) {
    expandedItems.sites[key] = false
  }
  expandedItems.sites[key] = !expandedItems.sites[key]
}

// Helper functions to check if items are expanded
const isServerExpanded = (serverIndex) => {
  return expandedItems.servers[serverIndex] || false
}

const isBindingExpanded = (serverIndex, bindingIndex) => {
  const key = `${serverIndex}-${bindingIndex}`
  return expandedItems.bindings[key] || false
}

const isSiteExpanded = (serverIndex, bindingIndex, siteIndex) => {
  const key = `${serverIndex}-${bindingIndex}-${siteIndex}`
  return expandedItems.sites[key] || false
}

// Add new server
const addServer = () => {
  if (!config.value.servers) {
    config.value.servers = []
  }
  config.value.servers.push({
    bindings: [{
      ip: "0.0.0.0",
      port: 80,
      is_admin: false,
      sites: [{
        hostnames: ["*"],
        is_default: true,
        is_enabled: true,
        is_ssl: false,
        is_ssl_required: false,
        web_root: "./www-default/",
        web_root_index_file_list: ["index.html"]
      }]
    }]
  })
}

// Remove server
const removeServer = (index) => {
  if (config.value.servers && config.value.servers.length > index) {
    config.value.servers.splice(index, 1)
  }
}

// Add new binding to server
const addBinding = (serverIndex) => {
  if (config.value.servers && config.value.servers[serverIndex]) {
    config.value.servers[serverIndex].bindings.push({
      ip: "0.0.0.0",
      port: 80,
      is_admin: false,
      sites: [{
        hostnames: ["*"],
        is_default: false,
        is_enabled: true,
        is_ssl: false,
        is_ssl_required: false,
        web_root: "./www-default/",
        web_root_index_file_list: ["index.html"]
      }]
    })
  }
}

// Remove binding from server
const removeBinding = (serverIndex, bindingIndex) => {
  if (config.value.servers && config.value.servers[serverIndex] &&
      config.value.servers[serverIndex].bindings.length > bindingIndex) {
    config.value.servers[serverIndex].bindings.splice(bindingIndex, 1)
  }
}

// Add new site to binding
const addSite = (serverIndex, bindingIndex) => {
  if (config.value.servers && config.value.servers[serverIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex]) {
    config.value.servers[serverIndex].bindings[bindingIndex].sites.push({
      hostnames: ["example.com"],
      is_default: false,
      is_enabled: true,
      is_ssl: false,
      is_ssl_required: false,
      web_root: "./www-default/",
      web_root_index_file_list: ["index.html"]
    })
  }
}

// Remove site from binding
const removeSite = (serverIndex, bindingIndex, siteIndex) => {
  if (config.value.servers && config.value.servers[serverIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex].sites.length > siteIndex) {
    config.value.servers[serverIndex].bindings[bindingIndex].sites.splice(siteIndex, 1)
  }
}

// Add hostname to site
const addHostname = (serverIndex, bindingIndex, siteIndex) => {
  if (config.value.servers && config.value.servers[serverIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex]) {
    config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex].hostnames.push("example.com")
  }
}

// Remove hostname from site
const removeHostname = (serverIndex, bindingIndex, siteIndex, hostnameIndex) => {
  if (config.value.servers && config.value.servers[serverIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex].hostnames.length > hostnameIndex) {
    config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex].hostnames.splice(hostnameIndex, 1)
  }
}

// Add index file to site
const addIndexFile = (serverIndex, bindingIndex, siteIndex) => {
  if (config.value.servers && config.value.servers[serverIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex]) {
    config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex].web_root_index_file_list.push("index.html")
  }
}

// Remove index file from site
const removeIndexFile = (serverIndex, bindingIndex, siteIndex, fileIndex) => {
  if (config.value.servers && config.value.servers[serverIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex] &&
      config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex].web_root_index_file_list.length > fileIndex) {
    config.value.servers[serverIndex].bindings[bindingIndex].sites[siteIndex].web_root_index_file_list.splice(fileIndex, 1)
  }
}

// Initialize
onMounted(() => {
  loadConfiguration()
})
</script>

<template>
  <div :class="inline ? 'config-editor-inline' : 'config-editor'">
    <!-- Header -->
    <div v-if="!inline" class="config-header">
      <h2>Configuration Editor</h2>
      <div class="config-actions">
        <button
          v-if="hasUnsavedChanges"
          @click="resetChanges"
          class="reset-button"
          :disabled="isSaving"
        >
          Reset Changes
        </button>
        <button
          @click="saveConfiguration"
          class="save-button"
          :disabled="isSaving"
        >
          <span v-if="isSaving">Saving...</span>
          <span v-else>Save Configuration</span>
        </button>
        <button v-if="!inline" @click="emit('close')" class="close-button">
          Close
        </button>
      </div>
    </div>

    <!-- Inline Header -->
    <div v-if="inline" class="inline-config-header">
      <h2>Server Configuration</h2>
      <div class="inline-config-actions">
        <button
          v-if="hasUnsavedChanges"
          @click="resetChanges"
          class="reset-button inline"
          :disabled="isSaving"
        >
          Reset Changes
        </button>
        <button
          @click="saveConfiguration"
          class="save-button inline"
          :disabled="isSaving"
        >
          <span v-if="isSaving">Saving...</span>
          <span v-else>Save Configuration</span>
        </button>
      </div>
    </div>    <!-- Loading State -->
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

      <!-- Unsaved changes indicator -->
      <div v-if="hasUnsavedChanges" class="changes-indicator">
        You have unsaved changes
      </div>

      <!-- Servers Section -->
      <div class="config-section">
        <div class="section-header" @click="toggleSection('servers')">
          <span class="section-icon" :class="{ expanded: expandedSections.servers }">▶</span>
          <span class="section-title-icon">🖥️</span>
          <h3>Servers</h3>
          <button @click.stop="addServer" class="add-button">+ Add Server</button>
        </div>

        <div v-if="expandedSections.servers" class="section-content">
          <div v-if="!config.servers || config.servers.length === 0" class="empty-state-section">
            <div class="empty-icon">🖥️</div>
            <p>No servers configured</p>
            <button @click="addServer" class="add-button">+ Add First Server</button>
          </div>

          <!-- Servers List -->
          <div v-for="(server, serverIndex) in config.servers" :key="serverIndex" class="server-item">
            <div class="item-header compact" @click="toggleServer(serverIndex)">
              <div class="header-left">
                <span class="section-icon" :class="{ expanded: isServerExpanded(serverIndex) }">▶</span>
                <span class="hierarchy-indicator server-indicator">🖥️</span>
                <h4>Server {{ serverIndex + 1 }}</h4>
                <span class="item-summary">({{ server.bindings?.length || 0 }} bindings)</span>
              </div>
              <button @click.stop="removeServer(serverIndex)" class="remove-button compact" :disabled="config.servers.length === 1">Remove</button>
            </div>

            <!-- Server Content -->
            <div v-if="isServerExpanded(serverIndex)" class="item-content">
              <div class="subsection-header compact">
                <h5>🔌 Network Bindings</h5>
                <button @click="addBinding(serverIndex)" class="add-button small">+ Add Binding</button>
              </div>

              <div v-if="!server.bindings || server.bindings.length === 0" class="empty-state-section small">
                <div class="empty-icon">🔌</div>
                <p>No network bindings configured</p>
              </div>

              <!-- Bindings List -->
              <div v-for="(binding, bindingIndex) in server.bindings" :key="bindingIndex" class="binding-item">
                <div class="item-header compact" @click="toggleBinding(serverIndex, bindingIndex)">
                  <div class="header-left">
                    <span class="section-icon" :class="{ expanded: isBindingExpanded(serverIndex, bindingIndex) }">▶</span>
                    <span class="hierarchy-indicator binding-indicator">🔌</span>
                    <h6>Binding {{ bindingIndex + 1 }}</h6>
                    <span class="binding-summary">{{ binding.ip }}:{{ binding.port }}</span>
                    <span v-if="binding.is_admin" class="admin-badge">ADMIN</span>
                    <span class="item-summary">({{ binding.sites?.length || 0 }} sites)</span>
                  </div>
                  <button @click.stop="removeBinding(serverIndex, bindingIndex)" class="remove-button compact small" :disabled="server.bindings.length === 1">Remove</button>
                </div>

                <!-- Binding Content -->
                <div v-if="isBindingExpanded(serverIndex, bindingIndex)" class="item-content">
                  <div class="form-grid compact">
                    <div class="form-field small-field">
                      <label>IP Address</label>
                      <input v-model="binding.ip" type="text" />
                    </div>
                    <div class="form-field small-field">
                      <label>Port</label>
                      <input v-model.number="binding.port" type="number" min="1" max="65535" />
                    </div>
                  </div>

                  <div class="subsection-header compact">
                    <h6>🌐 Websites</h6>
                    <button @click="addSite(serverIndex, bindingIndex)" class="add-button small">+ Add Site</button>
                  </div>

                  <div v-if="!binding.sites || binding.sites.length === 0" class="empty-state-section small">
                    <div class="empty-icon">🌐</div>
                    <p>No websites configured</p>
                  </div>

                  <!-- Sites List -->
                  <div v-for="(site, siteIndex) in binding.sites" :key="siteIndex" class="site-item">
                    <div class="item-header compact" @click="toggleSite(serverIndex, bindingIndex, siteIndex)">
                      <div class="header-left">
                        <span class="section-icon" :class="{ expanded: isSiteExpanded(serverIndex, bindingIndex, siteIndex) }">▶</span>
                        <span class="hierarchy-indicator site-indicator">🌐</span>
                        <h6>Site {{ siteIndex + 1 }}</h6>
                        <span class="site-summary">{{ site.hostnames[0] || 'No hostname' }}</span>
                        <span v-if="site.is_default" class="default-badge">DEFAULT</span>
                        <span v-if="site.is_ssl" class="ssl-badge">SSL</span>
                      </div>
                      <button @click.stop="removeSite(serverIndex, bindingIndex, siteIndex)" class="remove-button compact small" :disabled="binding.sites.length === 1">Remove</button>
                    </div>

                    <!-- Site Content -->
                    <div v-if="isSiteExpanded(serverIndex, bindingIndex, siteIndex)" class="item-content">
                      <div class="form-grid compact">
                        <div class="form-field">
                          <label>Web Root</label>
                          <input v-model="site.web_root" type="text" />
                        </div>
                        <div class="form-field checkbox-grid compact">
                          <label>
                            <input v-model="site.is_default" type="checkbox" />
                            Default Site
                          </label>
                          <label>
                            <input v-model="site.is_enabled" type="checkbox" />
                            Enabled
                          </label>
                          <label>
                            <input v-model="site.is_ssl" type="checkbox" />
                            SSL Enabled
                          </label>
                          <label>
                            <input v-model="site.is_ssl_required" type="checkbox" />
                            SSL Required
                          </label>
                        </div>
                      </div>

                      <!-- Hostnames and Index Files in two columns -->
                      <div class="two-column-layout">
                        <!-- Hostnames -->
                        <div class="list-field compact half-width">
                          <label>Hostnames (use * to match all hostnames)</label>
                          <div class="list-items">
                            <div v-for="(hostname, hostnameIndex) in site.hostnames" :key="hostnameIndex" class="list-item">
                              <input v-model="site.hostnames[hostnameIndex]" type="text" />
                              <button @click="removeHostname(serverIndex, bindingIndex, siteIndex, hostnameIndex)" class="remove-item-button">×</button>
                            </div>
                            <button @click="addHostname(serverIndex, bindingIndex, siteIndex)" class="add-item-button">+ Add Hostname</button>
                          </div>
                        </div>

                        <!-- Index Files -->
                        <div class="list-field compact half-width">
                          <label>Index Files</label>
                          <div class="list-items">
                            <div v-for="(file, fileIndex) in site.web_root_index_file_list" :key="fileIndex" class="list-item">
                              <input v-model="site.web_root_index_file_list[fileIndex]" type="text" />
                              <button @click="removeIndexFile(serverIndex, bindingIndex, siteIndex, fileIndex)" class="remove-item-button">×</button>
                            </div>
                            <button @click="addIndexFile(serverIndex, bindingIndex, siteIndex)" class="add-item-button">+ Add Index File</button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Admin Site Section -->
      <div class="config-section">
        <div class="section-header" @click="toggleSection('adminSite')">
          <span class="section-icon" :class="{ expanded: expandedSections.adminSite }">▶</span>
          <span class="section-title-icon">⚙️</span>
          <h3>Admin Portal Settings</h3>
        </div>

        <div v-if="expandedSections.adminSite" class="section-content">
          <div class="form-grid compact admin-portal-layout">
            <div class="form-field full-width">
              <label>
                <input v-model="config.admin_site.is_admin_portal_enabled" type="checkbox" />
                Enable Admin Portal (Beware that disabling this will prevent access to the admin dashboard after saving)
              </label>
            </div>
            <div class="form-field small-field">
              <label>IP Address</label>
              <input v-model="config.admin_site.admin_portal_ip" type="text" />
            </div>
            <div class="form-field small-field">
              <label>Port</label>
              <input v-model.number="config.admin_site.admin_portal_port" type="number" min="1" max="65535" />
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
.ssl-badge {
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

.ssl-badge {
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
  min-height:40px;
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
}

.form-field.checkbox-grid.compact {
  padding: 0.75rem;
  gap: 0.75rem;
}

.list-field.compact {
  margin: 1rem;
  padding: 1rem;
}

.remove-button.compact {
  padding: 0.5rem 0.75rem;
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
  justify-content: space-between;
  align-items: center;
  padding: 0 0 1.5rem 0;
  background: transparent;
  color: #1e293b;
  border-bottom: 2px solid #e2e8f0;
  margin-bottom: 1.5rem;
}

.config-header h2 {
  margin: 0;
  color: white;
  font-size: 1.5rem;
  font-weight: 700;
}

.inline-config-header h2 {
  margin: 0;
  color: #1e293b;
  font-size: 1.75rem;
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
  box-shadow: 0 2px 8px rgba(16, 185, 129, 0.3);
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
  box-shadow: 0 2px 8px rgba(245, 158, 11, 0.3);
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
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
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

.form-field input[type="text"],
.form-field input[type="number"] {
  padding: 0.875rem;
  border: 2px solid #e5e7eb;
  border-radius: 8px;
  font-size: 0.875rem;
  transition: all 0.2s ease;
  background: white;
}

.form-field input[type="text"]:focus,
.form-field input[type="number"]:focus {
  outline: none;
  border-color: #667eea;
  box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.1);
}

.form-field input[type="checkbox"] {
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
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
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

/* Admin portal specific layout */
.admin-portal-layout {
  display: grid;
  grid-template-columns: 140px 140px 1fr;
  grid-template-areas:
    "checkbox checkbox checkbox"
    "ip port .";
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

@media (max-width: 768px) {
  .two-column-layout {
    grid-template-columns: 1fr;
  }

  .admin-portal-layout {
    grid-template-columns: 1fr;
    grid-template-areas:
      "checkbox"
      "ip"
      "port";
  }
}
</style>
