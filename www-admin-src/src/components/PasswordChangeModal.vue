<script setup>
import { ref, reactive } from 'vue';

const emit = defineEmits(['close', 'password-changed']);

const form = reactive({
    oldPassword: '',
    newPassword: '',
    confirmPassword: '',
});

const isLoading = ref(false);
const error = ref('');
const showOldPassword = ref(false);
const showNewPassword = ref(false);
const showConfirmPassword = ref(false);
const successMessage = ref('');

const validateForm = () => {
    error.value = '';
    if (!form.oldPassword) {
        error.value = 'Current password is required';
        return false;
    }
    if (!form.newPassword || form.newPassword.length < 8) {
        error.value = 'New password must be at least 8 characters';
        return false;
    }
    if (form.newPassword !== form.confirmPassword) {
        error.value = 'New passwords do not match';
        return false;
    }
    if (form.oldPassword === form.newPassword) {
        error.value = 'New password must be different from current password';
        return false;
    }
    return true;
};

const handleSubmit = async () => {
    if (!validateForm()) return;

    isLoading.value = true;
    error.value = '';
    successMessage.value = '';

    try {
        const token = localStorage.getItem('gruxi_session_token');
        const response = await fetch('/user/password', {
            method: 'POST',
            headers: {
                Authorization: `Bearer ${token}`,
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                old_password: form.oldPassword,
                new_password: form.newPassword,
            }),
        });

        const data = await response.json();

        if (response.ok && data.success) {
            successMessage.value = 'Password changed successfully. You will be logged out.';
            setTimeout(() => {
                emit('password-changed');
                emit('close');
            }, 1500);
        } else {
            error.value = data.error || 'Failed to change password';
        }
    } catch (err) {
        console.error('Password change error:', err);
        error.value = 'Network error. Please try again.';
    } finally {
        isLoading.value = false;
    }
};
</script>

<template>
    <div class="modal-overlay" @click.self="emit('close')">
        <div class="modal-content">
            <div class="modal-header">
                <h2>Change Password</h2>
                <button class="close-btn" @click="emit('close')" :disabled="isLoading">&#x2715;</button>
            </div>

            <form @submit.prevent="handleSubmit" class="password-form">
                <!-- Current Password -->
                <div class="form-group">
                    <label for="old-password">Current Password</label>
                    <div class="password-input-wrapper">
                        <input
                            id="old-password"
                            v-model="form.oldPassword"
                            :type="showOldPassword ? 'text' : 'password'"
                            placeholder="Enter your current password"
                            :disabled="isLoading"
                            autocomplete="current-password"
                        />
                        <button type="button" class="toggle-visibility-btn" @click="showOldPassword = !showOldPassword" :disabled="isLoading" :title="showOldPassword ? 'Hide password' : 'Show password'">
                            <svg v-if="showOldPassword" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/><path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/><line x1="1" y1="1" x2="23" y2="23"/></svg>
                            <svg v-else xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
                        </button>
                    </div>
                </div>

                <!-- New Password -->
                <div class="form-group">
                    <label for="new-password">New Password</label>
                    <div class="password-input-wrapper">
                        <input
                            id="new-password"
                            v-model="form.newPassword"
                            :type="showNewPassword ? 'text' : 'password'"
                            placeholder="Min 8 characters"
                            :disabled="isLoading"
                            autocomplete="new-password"
                        />
                        <button type="button" class="toggle-visibility-btn" @click="showNewPassword = !showNewPassword" :disabled="isLoading" :title="showNewPassword ? 'Hide password' : 'Show password'">
                            <svg v-if="showNewPassword" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/><path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/><line x1="1" y1="1" x2="23" y2="23"/></svg>
                            <svg v-else xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
                        </button>
                    </div>
                </div>

                <!-- Confirm New Password -->
                <div class="form-group">
                    <label for="confirm-password">Confirm New Password</label>
                    <div class="password-input-wrapper">
                        <input
                            id="confirm-password"
                            v-model="form.confirmPassword"
                            :type="showConfirmPassword ? 'text' : 'password'"
                            placeholder="Re-enter new password"
                            :disabled="isLoading"
                            autocomplete="new-password"
                        />
                        <button type="button" class="toggle-visibility-btn" @click="showConfirmPassword = !showConfirmPassword" :disabled="isLoading" :title="showConfirmPassword ? 'Hide password' : 'Show password'">
                            <svg v-if="showConfirmPassword" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/><path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/><line x1="1" y1="1" x2="23" y2="23"/></svg>
                            <svg v-else xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
                        </button>
                    </div>
                </div>

                <!-- Error Message -->
                <div v-if="error" class="error-message">{{ error }}</div>

                <!-- Success Message -->
                <div v-if="successMessage" class="success-message">{{ successMessage }}</div>

                <!-- Buttons -->
                <div class="modal-buttons">
                    <button type="button" class="btn-secondary" @click="emit('close')" :disabled="isLoading">Cancel</button>
                    <button type="submit" class="btn-primary" :disabled="isLoading || !form.oldPassword || !form.newPassword || !form.confirmPassword">
                        {{ isLoading ? 'Changing...' : 'Change Password' }}
                    </button>
                </div>
            </form>
        </div>
    </div>
</template>

<style scoped>
.modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 2000;
}

.modal-content {
    background: white;
    border-radius: 12px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
    width: 100%;
    max-width: 420px;
    overflow: hidden;
}

.modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1.5rem;
    border-bottom: 1px solid #e5e7eb;
}

.modal-header h2 {
    font-size: 1.25rem;
    font-weight: 600;
    color: #1f2937;
    margin: 0;
}

.close-btn {
    background: none;
    border: none;
    font-size: 1.25rem;
    cursor: pointer;
    color: #6b7280;
    padding: 0;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    transition: all 0.2s;
}

.close-btn:hover:not(:disabled) {
    background: #f3f4f6;
    color: #1f2937;
}

.close-btn:disabled {
    cursor: not-allowed;
    opacity: 0.5;
}

.password-form {
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
}

.form-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
}

.form-group label {
    font-weight: 600;
    color: #1f2937;
    font-size: 0.875rem;
}

.password-input-wrapper {
    display: flex;
    align-items: center;
    border: 2px solid #e5e7eb;
    border-radius: 8px;
    transition: all 0.2s;
}

.password-input-wrapper:focus-within {
    border-color: #3b82f6;
    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
}

.password-input-wrapper input {
    flex: 1;
    padding: 0.625rem 0.75rem;
    border: none;
    font-size: 0.95rem;
    background: transparent;
    outline: none;
    color: #1f2937;
}

.password-input-wrapper input::placeholder {
    color: #9ca3af;
}

.password-input-wrapper input:disabled {
    cursor: not-allowed;
}

.toggle-visibility-btn {
    background: none;
    border: none;
    cursor: pointer;
    padding: 0.5rem 0.75rem;
    color: #6b7280;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.2s;
}

.toggle-visibility-btn:hover:not(:disabled) {
    color: #1f2937;
}

.toggle-visibility-btn:disabled {
    cursor: not-allowed;
    opacity: 0.5;
}

.error-message {
    background: #fee2e2;
    border: 1px solid #fecaca;
    color: #dc2626;
    padding: 0.75rem 1rem;
    border-radius: 8px;
    font-size: 0.875rem;
    text-align: center;
}

.success-message {
    background: #d1fae5;
    border: 1px solid #a7f3d0;
    color: #047857;
    padding: 0.75rem 1rem;
    border-radius: 8px;
    font-size: 0.875rem;
    text-align: center;
}

.modal-buttons {
    display: flex;
    gap: 0.75rem;
    padding-top: 0.5rem;
}

.btn-primary,
.btn-secondary {
    flex: 1;
    padding: 0.75rem 1rem;
    border-radius: 8px;
    border: none;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
    font-size: 0.9rem;
}

.btn-primary {
    background: #3b82f6;
    color: white;
}

.btn-primary:hover:not(:disabled) {
    background: #2563eb;
}

.btn-primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
}

.btn-secondary {
    background: #f3f4f6;
    color: #1f2937;
    border: 1px solid #e5e7eb;
}

.btn-secondary:hover:not(:disabled) {
    background: #e5e7eb;
}

.btn-secondary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
}
</style>
