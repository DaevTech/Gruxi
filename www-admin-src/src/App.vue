<script setup>
import { ref, reactive, onMounted } from 'vue'
import { apiFetch } from './api'
import LoginForm from './components/LoginForm.vue'
import AdminDashboard from './components/AdminDashboard.vue'

// Authentication state
const isAuthenticated = ref(false)
const isLoading = ref(true)
const user = reactive({
  username: '',
  sessionToken: ''
})

// Check for existing session on app load
onMounted(async () => {
  try {
    const response = await apiFetch('/basic', {
      method: 'GET'
    })

    if (response.ok) {
      const data = await response.json()
      user.username = data.username || ''
      isAuthenticated.value = true
    }
  } catch (error) {
    console.error('Error verifying session:', error)
  }

  isLoading.value = false
})

// Handle successful login
const handleLoginSuccess = (loginData) => {
  user.username = loginData.username
  user.sessionToken = ''
  isAuthenticated.value = true
}

// Handle logout
const handleLogout = async () => {
  try {
    await apiFetch('/logout', {
      method: 'POST'
    })
  } catch (error) {
    console.error('Error during logout:', error)
  } finally {
    // Clear local state regardless of API response
    user.username = ''
    user.sessionToken = ''
    isAuthenticated.value = false
  }
}
</script>

<template>
  <div id="app">
    <!-- Loading state -->
    <div v-if="isLoading" class="loading-container">
      <div class="loading-spinner"></div>
      <p>Loading Gruxi Admin...</p>
    </div>

    <!-- Login form when not authenticated -->
    <LoginForm
      v-else-if="!isAuthenticated"
      @login-success="handleLoginSuccess"
    />

    <!-- Admin dashboard when authenticated -->
    <AdminDashboard
      v-else
      :user="user"
      @logout="handleLogout"
    />
  </div>
</template>

<style scoped>
.loading-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  gap: 1rem;
}

.loading-spinner {
  width: 40px;
  height: 40px;
  border: 4px solid #f3f3f3;
  border-top: 4px solid #646cff;
  border-radius: 50%;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

#app {
  min-height: 100vh;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
}
</style>
