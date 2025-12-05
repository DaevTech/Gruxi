<script setup>
import { ref, onMounted } from 'vue'

const props = defineProps({
  user: {
    type: Object,
    required: true
  }
})

const currentMode = ref('PRODUCTION')
const isLoading = ref(true)
const isChanging = ref(false)
const errorMessage = ref('')

const modes = [
  { value: 'DEV', label: 'Development', color: '#10b981' },
  { value: 'DEBUG', label: 'Debug', color: '#f59e0b' },
  { value: 'PRODUCTION', label: 'Production', color: '#3b82f6' },
  { value: 'SPEEDTEST', label: 'Speed Test', color: '#8b5cf6' }
]

// Fetch current operation mode
const fetchOperationMode = async () => {
  try {
    const token = localStorage.getItem('grux_session_token')
    if (!token) {
      console.error('No session token available')
      return
    }

    const response = await fetch('/operation-mode', {
      method: 'GET',
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
      }
    })

    if (response.ok) {
      const data = await response.json()
      currentMode.value = data.mode
      errorMessage.value = ''
    } else {
      console.error('Failed to fetch operation mode:', response.status)
      errorMessage.value = 'Failed to load mode'
    }
  } catch (error) {
    console.error('Error fetching operation mode:', error)
    errorMessage.value = 'Connection error'
  } finally {
    isLoading.value = false
  }
}

// Change operation mode
const changeOperationMode = async (event) => {
  const newMode = event.target.value

  isChanging.value = true
  errorMessage.value = ''

  try {
    const token = localStorage.getItem('grux_session_token')
    if (!token) {
      errorMessage.value = 'Not authenticated'
      isChanging.value = false
      return
    }

    const response = await fetch('/operation-mode', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ mode: newMode })
    })

    if (response.ok) {
      const data = await response.json()
      console.log('Operation mode changed:', data.message)
    } else {
      const errorData = await response.json()
      errorMessage.value = errorData.error || 'Failed to change mode'
      console.error('Failed to change operation mode:', errorData)
      // Revert to previous mode on error
      await fetchOperationMode()
    }
  } catch (error) {
    console.error('Error changing operation mode:', error)
    errorMessage.value = 'Connection error'
    // Revert to previous mode on error
    await fetchOperationMode()
  } finally {
    isChanging.value = false
  }
}

// Initialize on mount
onMounted(() => {
  fetchOperationMode()
})

// Get current mode color
const getCurrentModeColor = () => {
  const mode = modes.find(m => m.value === currentMode.value)
  return mode ? mode.color : '#3b82f6'
}
</script>

<template>
  <div class="operation-mode-selector">
    <label class="mode-label">Operation Mode <span class="help-icon" data-tooltip="Operation mode determines the level of system logging and performance characteristics. DEV being the most verbose and SPEEDTEST being the least logging. For normal use, PRODUCTION mode is recommended.">?</span></label>

    <div v-if="isLoading" class="mode-loading">
      <div class="loading-spinner-small"></div>
    </div>

    <select
      v-else
      v-model="currentMode"
      @change="changeOperationMode"
      :disabled="isChanging"
      class="mode-select"
      :style="{ borderLeftColor: getCurrentModeColor() }"
    >
      <option
        v-for="mode in modes"
        :key="mode.value"
        :value="mode.value"
      >
        {{ mode.label }}
      </option>
    </select>

    <div v-if="errorMessage" class="error-message">
      {{ errorMessage }}
    </div>

    <div v-if="isChanging" class="changing-indicator">
      Updating...
    </div>
  </div>
</template>

<style scoped>
.operation-mode-selector {
  padding: 0;
  margin-bottom: 1rem;
}

.mode-label {
  display: block;
  font-size: 0.75rem;
  font-weight: 600;
  color: #9ca3af;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 0.5rem;
}

.mode-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0.5rem;
}

.loading-spinner-small {
  width: 20px;
  height: 20px;
  border: 2px solid #4b5563;
  border-top: 2px solid #3b82f6;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

.mode-select {
  width: 100%;
  padding: 0.625rem 0.75rem;
  background: #374151;
  color: white;
  border: 1px solid #4b5563;
  border-left: 3px solid #3b82f6;
  border-radius: 6px;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
  outline: none;
}

.mode-select:hover:not(:disabled) {
  background: #4b5563;
  border-color: #6b7280;
}

.mode-select:focus {
  border-color: #3b82f6;
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
}

.mode-select:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.mode-select option {
  background: #1f2937;
  color: white;
  padding: 0.5rem;
}

.error-message {
  margin-top: 0.5rem;
  padding: 0.375rem 0.5rem;
  background: rgba(239, 68, 68, 0.1);
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 4px;
  color: #fca5a5;
  font-size: 0.75rem;
  font-weight: 500;
}

.changing-indicator {
  margin-top: 0.5rem;
  padding: 0.25rem;
  text-align: center;
  color: #60a5fa;
  font-size: 0.75rem;
  font-weight: 500;
  font-style: italic;
}
</style>
