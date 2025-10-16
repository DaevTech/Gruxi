<script setup>
import { ref, reactive, onMounted, nextTick } from 'vue'

// Define props
const props = defineProps({
  user: {
    type: Object,
    required: true
  }
})

// Component state
const isLoading = ref(false)
const error = ref('')
const logFiles = ref([])
const selectedLog = ref('')
const logContent = ref('')
const logContentContainer = ref(null)
const logInfo = reactive({
  filename: '',
  fileSize: 0,
  isTruncated: false,
  fullPath: '',
  message: ''
})

// Load log files on mount
onMounted(async () => {
  await loadLogFiles()

  // Auto-select system.log if it exists
  const systemLog = logFiles.value.find(file => file.filename === 'system.log')
  if (systemLog) {
    selectedLog.value = systemLog.filename
    await loadLogContent(systemLog.filename)
  }
})

// Load the list of available log files
const loadLogFiles = async () => {
  isLoading.value = true
  error.value = ''

  try {
    const response = await fetch('/logs', {
      method: 'GET',
      headers: {
        'Authorization': `Bearer ${props.user.sessionToken}`,
        'Content-Type': 'application/json'
      }
    })

    if (response.ok) {
      const data = await response.json()
      if (data.success) {
        logFiles.value = data.files.sort((a, b) => {
          // Sort with system.log first, then alphabetically
          if (a.filename === 'system.log') return -1
          if (b.filename === 'system.log') return 1
          return a.filename.localeCompare(b.filename)
        })
      } else {
        error.value = 'Failed to load log files'
      }
    } else {
      const errorData = await response.json()
      error.value = errorData.error || 'Failed to load log files'
    }
  } catch (err) {
    console.error('Error loading log files:', err)
    error.value = 'Network error: Failed to load log files'
  } finally {
    isLoading.value = false
  }
}

// Load the content of a specific log file
const loadLogContent = async (filename) => {
  if (!filename) return

  isLoading.value = true
  error.value = ''
  logContent.value = ''

  try {
    const response = await fetch(`/logs/${filename}`, {
      method: 'GET',
      headers: {
        'Authorization': `Bearer ${props.user.sessionToken}`,
        'Content-Type': 'application/json'
      }
    })

    if (response.ok) {
      const data = await response.json()
      if (data.success) {
        logContent.value = data.content
        logInfo.filename = data.filename
        logInfo.fileSize = data.file_size
        logInfo.isTruncated = data.is_truncated
        logInfo.fullPath = data.full_path
        logInfo.message = data.message
      } else {
        error.value = 'Failed to load log content'
      }
    } else {
      const errorData = await response.json()
      error.value = errorData.error || 'Failed to load log content'
    }
  } catch (err) {
    console.error('Error loading log content:', err)
    error.value = 'Network error: Failed to load log content'
  } finally {
    isLoading.value = false
  }
}

// Handle log file selection change
const onLogFileChange = () => {
  if (selectedLog.value) {
    loadLogContent(selectedLog.value)
  }
}

// Refresh logs - refresh both log files list and current log content
const refreshLog = async () => {
  // Always refresh the log files list first
  await loadLogFiles()

  // If a log is selected, refresh its content
  if (selectedLog.value) {
    await loadLogContent(selectedLog.value)
  }
}

// Scroll to bottom of log content
const scrollToBottom = async () => {
  await nextTick()
  if (logContentContainer.value) {
    logContentContainer.value.scrollTop = logContentContainer.value.scrollHeight
  }
}

// Format file size for display
const formatFileSize = (bytes) => {
  if (bytes === 0) return '0 Bytes'
  const k = 1024
  const sizes = ['Bytes', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}
</script>

<template>
  <div class="logs-container">

    <!-- Error message -->
    <div v-if="error" class="error-message">
      {{ error }}
    </div>

    <!-- Log file selector -->
    <div class="log-selector">
      <label for="logSelect">Select Log File:</label>
      <select
        id="logSelect"
        v-model="selectedLog"
        @change="onLogFileChange"
        :disabled="isLoading"
      >
        <option value="">-- Select a log file --</option>
        <option
          v-for="file in logFiles"
          :key="file.filename"
          :value="file.filename"
        >
          {{ file.filename }} ({{ formatFileSize(file.size) }})
        </option>
      </select>

       <button
          @click="scrollToBottom"
          :disabled="!logContent"
          class="scroll-btn"
          title="Scroll to bottom of log"
        >
          ↓ Bottom
        </button>
        <button
          @click="refreshLog"
          :disabled="isLoading"
          class="refresh-btn"
        >
          {{ isLoading ? 'Loading...' : 'Refresh' }}
        </button>
    </div>

    <!-- Log info -->
    <div v-if="logInfo.message.length > 0" class="log-info">
      <div class="info-item warning">
        <strong>Note:</strong> {{ logInfo.message }}
      </div>
    </div>

    <!-- Log content -->
    <div ref="logContentContainer" class="log-content-container">
      <div v-if="isLoading" class="loading">
        Loading log content...
      </div>
      <div v-else-if="logContent" class="log-content">
        <pre>{{ logContent }}</pre>
      </div>
      <div v-else-if="selectedLog" class="empty-content">
        Log file is empty or could not be read.
      </div>
      <div v-else class="no-selection">
        Select a log file to view its content.
      </div>
    </div>
  </div>
</template>

<style scoped>
.logs-container {
  padding: 1rem;
  max-width: 100%;
}

.logs-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1rem;
  padding-bottom: 0.5rem;
  border-bottom: 2px solid #e2e8f0;
}

.logs-header h2 {
  color: #2d3748;
  margin: 0;
}

.header-buttons {
  display: flex;
  gap: 0.5rem;
}

.refresh-btn {
  padding: 0.5rem 1rem;
  background-color: #4299e1;
  color: white;
  border: none;
  border-radius: 0.375rem;
  cursor: pointer;
  font-weight: 500;
  transition: background-color 0.2s;
}

.refresh-btn:hover:not(:disabled) {
  background-color: #3182ce;
}

.refresh-btn:disabled {
  background-color: #a0aec0;
  cursor: not-allowed;
}

.scroll-btn {
  padding: 0.5rem 0.75rem;
  background-color: #48bb78;
  color: white;
  border: none;
  border-radius: 0.375rem;
  cursor: pointer;
  font-weight: 500;
  transition: background-color 0.2s;
  font-size: 0.875rem;
}

.scroll-btn:hover:not(:disabled) {
  background-color: #38a169;
}

.scroll-btn:disabled {
  background-color: #a0aec0;
  cursor: not-allowed;
}

.error-message {
  background-color: #fed7d7;
  border: 1px solid #fc8181;
  color: #c53030;
  padding: 0.75rem;
  border-radius: 0.375rem;
  margin-bottom: 1rem;
}

.log-selector {
  margin-bottom: 1rem;
}

.log-selector label {
  display: block;
  margin-bottom: 0.5rem;
  font-weight: 500;
  color: #4a5568;
}

.log-selector select {
  width: 100%;
  max-width: 400px;
  padding: 0.5rem;
  border: 1px solid #d2d6dc;
  border-radius: 0.375rem;
  background-color: white;
  color: #374151;
  font-size: 0.875rem;
}

.log-selector select:focus {
  outline: none;
  border-color: #4299e1;
  box-shadow: 0 0 0 3px rgba(66, 153, 225, 0.1);
}

.log-selector button {
  margin-left: 0.5rem;
}

.log-info {
  background-color: #f7fafc;
  border: 1px solid #e2e8f0;
  border-radius: 0.375rem;
  padding: 1rem;
  margin-bottom: 1rem;
}

.info-item {
  margin-bottom: 0.5rem;
  font-size: 0.875rem;
}

.info-item:last-child {
  margin-bottom: 0;
}

.info-item.warning {
  color: #d69e2e;
  font-weight: 500;
}

.log-content-container {
  border: 1px solid #d2d6dc;
  border-radius: 0.375rem;
  background-color: #1a202c;
  min-height: 400px;
  max-height: 600px;
  overflow: auto;
}

.loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 400px;
  color: #a0aec0;
  font-size: 1rem;
}

.log-content {
  padding: 1rem;
}

.log-content pre {
  margin: 0;
  color: #e2e8f0;
  font-family: 'Courier New', monospace;
  font-size: 0.875rem;
  line-height: 1.4;
  white-space: pre-wrap;
  word-break: break-word;
}

.empty-content,
.no-selection {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 400px;
  color: #a0aec0;
  font-size: 1rem;
  font-style: italic;
}

/* Scrollbar styling */
.log-content-container::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

.log-content-container::-webkit-scrollbar-track {
  background: #2d3748;
}

.log-content-container::-webkit-scrollbar-thumb {
  background: #4a5568;
  border-radius: 4px;
}

.log-content-container::-webkit-scrollbar-thumb:hover {
  background: #718096;
}
</style>