<template>
    <div class="tag-field">
        <span v-for="(item, index) in modelValue" :key="index" class="tag-item">
            {{ item }}
            <button @click="removeItem(index)" class="tag-remove-button" type="button">×</button>
        </span>
        <input
            type="text"
            class="tag-input"
            :placeholder="placeholder"
            @keydown.enter.prevent="handleEnter"
            ref="inputRef"
        />
    </div>
</template>

<script setup>
import { ref } from 'vue';

const props = defineProps({
    modelValue: {
        type: Array,
        required: true,
    },
    placeholder: {
        type: String,
        default: 'Add item and press Enter...',
    },
    defaultValue: {
        type: String,
        default: '',
    },
});

const emit = defineEmits(['update:modelValue']);

const inputRef = ref(null);

const handleEnter = (e) => {
    const value = e.target.value.trim();
    if (value) {
        const newArray = [...props.modelValue, value];
        emit('update:modelValue', newArray);
        e.target.value = '';
    }
};

const removeItem = (index) => {
    const newArray = [...props.modelValue];
    newArray.splice(index, 1);
    emit('update:modelValue', newArray);
};
</script>

<style scoped>
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
</style>
