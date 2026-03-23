<script setup>
import { ref } from 'vue';
import PasswordChangeModal from './PasswordChangeModal.vue';

const props = defineProps({
    user: {
        type: Object,
        required: true,
    },
});

const emit = defineEmits(['logout']);

const showPasswordModal = ref(false);

const handlePasswordChanged = () => {
    // Password was changed and all sessions invalidated — log the user out
    emit('logout');
};
</script>

<template>
    <div class="user-profile">
        <div class="profile-card">
            <div class="profile-header">
                <div class="avatar-large">👤</div>
                <div class="user-info-section">
                    <h1>{{ user.username }}</h1>
                    <p class="user-role">Administrator</p>
                </div>
            </div>

            <div class="profile-section">
                <h3>Security</h3>
                <button class="btn-change-password" @click="showPasswordModal = true">
                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
                    Change Password
                </button>
            </div>
        </div>

        <PasswordChangeModal v-if="showPasswordModal" @close="showPasswordModal = false" @password-changed="handlePasswordChanged" />
    </div>
</template>

<style scoped>
.user-profile {
    max-width: 600px;
}

.profile-card {
    background: white;
    border-radius: 12px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    border: 1px solid #e5e7eb;
    overflow: hidden;
}

.profile-header {
    display: flex;
    align-items: center;
    gap: 1.5rem;
    padding: 2rem;
    background: linear-gradient(135deg, #1e40af 0%, #3b82f6 100%);
}

.avatar-large {
    font-size: 3rem;
    width: 80px;
    height: 80px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.2);
    border-radius: 50%;
    flex-shrink: 0;
}

.user-info-section h1 {
    font-size: 1.5rem;
    font-weight: 700;
    color: white;
    margin: 0 0 0.25rem 0;
}

.user-role {
    color: rgba(255, 255, 255, 0.8);
    margin: 0;
    font-size: 0.95rem;
}

.profile-section {
    padding: 1.5rem 2rem;
    border-top: 1px solid #e5e7eb;
}

.profile-section h3 {
    margin: 0 0 1rem 0;
    font-size: 0.8rem;
    font-weight: 600;
    color: #6b7280;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.setting-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.625rem 0;
}

.setting-item + .setting-item {
    border-top: 1px solid #f3f4f6;
}

.setting-label {
    font-size: 0.9rem;
    color: #6b7280;
}

.setting-value {
    font-size: 0.9rem;
    font-weight: 600;
    color: #1f2937;
}

.btn-change-password {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.625rem 1.25rem;
    background: #f3f4f6;
    color: #1f2937;
    border: 1px solid #e5e7eb;
    border-radius: 8px;
    font-weight: 600;
    font-size: 0.875rem;
    cursor: pointer;
    transition: all 0.2s;
}

.btn-change-password:hover {
    background: #e5e7eb;
}
</style>
