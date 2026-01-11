<script setup>
import { ref, reactive, onMounted } from 'vue'
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
  const savedToken = localStorage.getItem('gruxi_session_token')
  const savedUsername = localStorage.getItem('gruxi_username')

  if (savedToken && savedUsername) {
    // Verify the token is still valid by making a test request
    try {
      const response = await fetch('/config', {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${savedToken}`,
          'Content-Type': 'application/json'
        }
      })

      if (response.ok) {
        user.sessionToken = savedToken
        user.username = savedUsername
        isAuthenticated.value = true
      } else {
        // Token is invalid, clear it
        localStorage.removeItem('gruxi_session_token')
        localStorage.removeItem('gruxi_username')
      }
    } catch (error) {
      console.error('Error verifying session:', error)
      localStorage.removeItem('gruxi_session_token')
      localStorage.removeItem('gruxi_username')
    }
  }

  isLoading.value = false
})

// Handle successful login
const handleLoginSuccess = (loginData) => {
  user.username = loginData.username
  user.sessionToken = loginData.session_token
  isAuthenticated.value = true

  // Save to localStorage
  localStorage.setItem('gruxi_session_token', loginData.session_token)
  localStorage.setItem('gruxi_username', loginData.username)
}

// Handle logout
const handleLogout = async () => {
  try {
    await fetch('/logout', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${user.sessionToken}`,
        'Content-Type': 'application/json'
      }
    })
  } catch (error) {
    console.error('Error during logout:', error)
  } finally {
    // Clear local state regardless of API response
    user.username = ''
    user.sessionToken = ''
    isAuthenticated.value = false

    // Clear localStorage
    localStorage.removeItem('gruxi_session_token')
    localStorage.removeItem('gruxi_username')
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
