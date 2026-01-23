<script setup>
import { ref, reactive, onMounted } from 'vue';
import LogViewer from './LogViewer.vue';
import ConfigurationEditor from './ConfigurationEditor.vue';
import OperationModeSelector from './OperationModeSelector.vue';

// Define props and emits
const props = defineProps({
    user: {
        type: Object,
        required: true,
    },
});

const emit = defineEmits(['logout']);

// Dashboard state
const activeView = ref('server-status');
const sidebarCollapsed = ref(false);

// Menu items
const menuItems = [
    { id: 'server-status', name: 'Server Status' },
    { id: 'configuration', name: 'Configuration' },
    { id: 'logs', name: 'Logs' },
];

// Server stats (real data from monitoring endpoint)
const stats = reactive({
    serverStatus: 'Loading...',
    uptime: '...',
    requests: 0,
    requestsPerSec: 0,
    activeConnections: 0,
    fileCache: {
        enabled: false,
        currentItems: 0,
        maxItems: 0,
    },
    lastUpdated: new Date(),
});

// Basic server info (from /basic endpoint)
const basicData = reactive({
    gruxiVersion: '...',
});

// Handle logout
const handleLogout = () => {
    emit('logout');
};

// Navigation
const setActiveView = (viewId) => {
    activeView.value = viewId;
};

// Toggle sidebar
const toggleSidebar = () => {
    sidebarCollapsed.value = !sidebarCollapsed.value;
};

// Function to fetch basic data from API
const updateBasicData = async () => {
    try {
        const token = props.user?.sessionToken || localStorage.getItem('gruxi_session_token');
        if (!token) {
            basicData.gruxiVersion = '...';
            return;
        }

        const response = await fetch('/basic', {
            method: 'GET',
            headers: {
                Authorization: `Bearer ${token}`,
                'Content-Type': 'application/json',
            },
        });

        if (response.ok) {
            const data = await response.json();
            basicData.gruxiVersion = data.gruxi_version || '...';
        } else if (response.status === 401) {
            emit('logout');
        } else {
            console.error('Failed to fetch basic data:', response.status);
        }
    } catch (error) {
        console.error('Error fetching basic data:', error);
    }
};

// Function to fetch real monitoring data from API
const updateStats = async () => {
    // First check if server is healthy
    const isHealthy = await checkHealth();

    if (!isHealthy) {
        stats.serverStatus = 'Unavailable';
        return;
    }

    try {
        const token = localStorage.getItem('gruxi_session_token');
        if (!token) {
            console.error('No session token available');
            stats.serverStatus = 'Running';
            return;
        }

        const response = await fetch('/monitoring', {
            method: 'GET',
            headers: {
                Authorization: `Bearer ${token}`,
                'Content-Type': 'application/json',
            },
        });

        if (response.ok) {
            const data = await response.json();

            // Update stats with real monitoring data
            stats.serverStatus = 'Running';
            stats.requests = data.requests_served || 0;
            stats.requestsPerSec = data.requests_per_sec || 0;
            stats.activeConnections = data.requests_in_progress || 0;

            // Update file cache stats
            if (data.file_cache) {
                stats.fileCache.enabled = data.file_cache.enabled || false;
                stats.fileCache.currentItems = data.file_cache.current_items || 0;
                stats.fileCache.maxItems = data.file_cache.max_items || 0;
            }

            // Convert uptime seconds to human readable format
            const uptimeSeconds = data.uptime_seconds || 0;
            const days = Math.floor(uptimeSeconds / (24 * 3600));
            const hours = Math.floor((uptimeSeconds % (24 * 3600)) / 3600);
            const minutes = Math.floor((uptimeSeconds % 3600) / 60);

            if (days > 0) {
                stats.uptime = `${days} day${days !== 1 ? 's' : ''}, ${hours} hour${hours !== 1 ? 's' : ''}`;
            } else if (hours > 0) {
                stats.uptime = `${hours} hour${hours !== 1 ? 's' : ''}, ${minutes} minute${minutes !== 1 ? 's' : ''}`;
            } else {
                stats.uptime = `${minutes} minute${minutes !== 1 ? 's' : ''}`;
            }

            stats.lastUpdated = new Date();
        } else if (response.status === 401) {
            // Session expired, redirect to login
            stats.serverStatus = 'Running'; // Server is up, just auth issue
            emit('logout');
        } else {
            console.error('Failed to fetch monitoring data:', response.status);
            stats.serverStatus = 'Running'; // Server responded, so it's running
        }
    } catch (error) {
        console.error('Error fetching monitoring data:', error);
        // If healthcheck passed but monitoring failed, server is still running
        stats.serverStatus = 'Running';
    }
};

// Function to check server health using the healthcheck endpoint
const checkHealth = async () => {
    try {
        const response = await fetch('/healthcheck', {
            method: 'GET',
        });

        if (response.ok) {
            return true; // Server is healthy
        }
        return false; // Server is not healthy
    } catch (error) {
        console.error('Health check failed:', error);
        return false; // Server is not reachable
    }
};

// Format request count with suffixes
const formatRequestCount = (count) => {
    if (count >= 1000000000) {
        return (count / 1000000000).toFixed(0) + 'B';
    } else if (count >= 1000000) {
        return (count / 1000000).toFixed(0) + 'M';
    } else if (count >= 1000) {
        return (count / 1000).toFixed(0) + 'K';
    } else {
        return count.toString();
    }
};

// Initialize dashboard
onMounted(() => {
    updateBasicData();
    updateStats();
    setInterval(updateStats, 10000); // Update stats every 10 seconds
});
</script>

<template>
    <div class="admin-layout">
        <!-- Left Sidebar -->
        <aside :class="['sidebar', { collapsed: sidebarCollapsed }]">
            <div class="sidebar-header">
                <div class="logo">
                    <span class="logo-icon"
                        ><svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="39 55 146 146">
                            <defs>
                                <linearGradient id="a" x1="48" y1="48" x2="208" y2="208" gradientUnits="userSpaceOnUse">
                                    <stop offset="0%" stop-color="#7C3AED" />
                                    <stop offset="100%" stop-color="#06B6D4" />
                                </linearGradient>
                            </defs>
                            <path d="M176 64H80l-32 64 32 64h96v-64h-48" stroke="url(#a)" stroke-width="18" stroke-linecap="round" stroke-linejoin="round" /></svg
                    ></span>
                    <span v-if="!sidebarCollapsed" class="logo-text">Gruxi admin</span>
                </div>
            </div>

            <nav class="sidebar-nav">
                <ul class="nav-list">
                    <li v-for="item in menuItems" :key="item.id" class="nav-item">
                        <button :class="['nav-link', { active: activeView === item.id }]" @click="setActiveView(item.id)">
                            <span v-if="!sidebarCollapsed" class="nav-text">
                                {{ item.name }}
                                <span v-if="item.id === 'server-status'" :class="['menu-status-indicator', stats.serverStatus === 'Running' ? 'online' : 'offline']">
                                    {{ stats.serverStatus === 'Running' ? 'Online' : 'Offline' }}
                                </span>
                            </span>
                            <span v-else class="nav-text-collapsed">{{ item.name.charAt(0) }}</span>
                        </button>
                    </li>
                </ul>
            </nav>

            <div v-if="!sidebarCollapsed" class="gruxi-version">Gruxi version: {{ basicData.gruxiVersion }}</div>
            <div class="sidebar-footer">
                <!-- Operation Mode Selector -->
                <OperationModeSelector v-if="!sidebarCollapsed" :user="user" />

                <div v-if="!sidebarCollapsed" class="footer-separator"></div>

                <div class="user-info" v-if="!sidebarCollapsed">
                    <div class="user-avatar">👤</div>
                    <div class="user-details">
                        <div class="user-name">{{ user.username }}</div>
                        <div class="user-role">Administrator</div>
                    </div>
                </div>
            </div>
        </aside>

        <!-- Main Content Area -->
        <div class="main-content">
            <!-- Top Header -->
            <header class="top-header">
                <div class="header-left">
                    <h1 class="page-title">
                        {{ menuItems.find((item) => item.id === activeView)?.name || 'Overview' }}
                    </h1>
                </div>
                <div class="header-right">
                    <button @click="handleLogout" class="logout-btn">
                        <span class="logout-text">Logout</span>
                    </button>
                </div>
            </header>

            <!-- Content Area -->
            <main class="content-area">
                <!-- Server Status -->
                <div v-if="activeView === 'server-status'" class="view-content">
                    <div class="stats-overview">
                        <div class="stats-row">
                            <div class="stat-card">
                                <div class="stat-header">
                                    <h3>Server Status</h3>
                                </div>
                                <div class="stat-value large">{{ stats.serverStatus }}</div>
                            </div>

                            <div class="stat-card">
                                <div class="stat-header">
                                    <h3>Uptime</h3>
                                </div>
                                <div class="stat-value">{{ stats.uptime }}</div>
                            </div>

                            <div class="stat-card">
                                <div class="stat-header">
                                    <h3>Requests Served</h3>
                                </div>
                                <div class="stat-value">{{ formatRequestCount(stats.requests) }}</div>
                                <div class="stat-subtitle">~ {{ stats.requestsPerSec }} req/sec right now</div>
                            </div>

                            <div class="stat-card">
                                <div class="stat-header">
                                    <h3>In-Progress requests</h3>
                                </div>
                                <div class="stat-value">{{ stats.activeConnections }}</div>
                            </div>
                        </div>
                        <div class="stats-row">
                            <div class="stat-card">
                                <div class="stat-header">
                                    <h3>File Cache</h3>
                                </div>
                                <div class="stat-value">
                                    {{ stats.fileCache.enabled ? `${stats.fileCache.currentItems} / ${stats.fileCache.maxItems}` : 'Disabled' }}
                                </div>
                                <div class="stat-subtitle">
                                    {{ stats.fileCache.enabled ? 'files cached' : '' }}
                                </div>
                            </div>
                            <div class="stat-card hidden"></div>
                            <div class="stat-card hidden"></div>
                            <div class="stat-card hidden"></div>
                        </div>
                    </div>
                </div>

                <!-- Logs View -->
                <div v-else-if="activeView === 'logs'" class="view-content">
                    <LogViewer :user="user" />
                </div>

                <!-- Configuration View -->
                <div v-else-if="activeView === 'configuration'" class="view-content">
                    <ConfigurationEditor :user="user" :inline="true" />
                </div>

                <!-- Other Views -->
                <div v-else class="view-content">
                    <div class="placeholder-content">
                        <div class="placeholder-icon">{{ menuItems.find((item) => item.id === activeView)?.icon || '�' }}</div>
                        <h2>{{ menuItems.find((item) => item.id === activeView)?.name || 'Page' }}</h2>
                        <p>This section is under development and will be implemented in future updates.</p>
                    </div>
                </div>
            </main>
        </div>
    </div>
</template>

<style scoped>
/* Main Layout */
.admin-layout {
    display: flex;
    height: 100vh;
    background: #f5f6fa;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

/* Sidebar */
.sidebar {
    width: 280px;
    background: #1f2937;
    color: white;
    display: flex;
    flex-direction: column;
    border-right: 1px solid #374151;
    transition: width 0.3s ease;
}

.sidebar.collapsed {
    width: 70px;
}

.sidebar-header {
    padding: 1.5rem;
    border-bottom: 1px solid #374151;
    display: flex;
    align-items: center;
    justify-content: space-between;
}

.logo {
    display: flex;
    align-items: center;
    gap: 0.75rem;
}

.logo-icon {
    width: 50px;
}

.logo-text {
    font-size: 1.25rem;
    font-weight: 700;
    white-space: nowrap;
    text-transform: uppercase;
}

.sidebar-nav {
    flex: 1;
    padding: 1rem 0;
}

.nav-list {
    list-style: none;
    padding: 0;
    margin: 0;
}

.nav-item {
    margin-bottom: 0.25rem;
}

.nav-link {
    display: flex;
    align-items: center;
    justify-content: left;
    width: 100%;
    padding: 0.75rem 1.5rem;
    background: transparent;
    border: none;
    color: #d1d5db;
    text-decoration: none;
    transition: all 0.2s;
    cursor: pointer;
    text-align: center;
}

.nav-link:hover {
    background: #374151;
    color: white;
}

.nav-link.active {
    background: #3b82f6;
    color: white;
}

.nav-text {
    font-weight: 500;
    white-space: nowrap;
    display: flex;
    align-items: center;
    width: 100%;
}

.nav-text-collapsed {
    font-weight: 600;
    font-size: 1.1rem;
    color: inherit;
}

.sidebar-footer {
    padding: 1.25rem 1.5rem;
    border-top: 1px solid #374151;
}

.gruxi-version {
    font-size: 0.75rem;
    color: #9ca3af;
    margin-bottom: 0.75rem;
    padding: 0rem 1.5rem;
}

.footer-separator {
    height: 1px;
    background: #374151;
    margin: 1rem -1.5rem 1rem -1.5rem;
}

.user-info {
    display: flex;
    align-items: center;
    gap: 0.75rem;
}

.user-avatar {
    width: 40px;
    height: 40px;
    background: #3b82f6;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.25rem;
    flex-shrink: 0;
}

.user-details {
    flex: 1;
    min-width: 0;
}

.user-name {
    font-weight: 600;
    font-size: 0.875rem;
    color: white;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.user-role {
    font-size: 0.75rem;
    color: #9ca3af;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

/* Main Content */
.main-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: #f5f6fa;
}

.top-header {
    background: white;
    border-bottom: 1px solid #e5e7eb;
    padding: 1rem 2rem;
    display: flex;
    align-items: center;
    justify-content: space-between;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.header-left {
    display: flex;
    align-items: center;
    gap: 0.75rem;
}

.page-title {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 700;
    color: #1f2937;
}

.header-right {
    display: flex;
    align-items: center;
    gap: 1rem;
}

.logout-btn {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1.25rem;
    background: #ef4444;
    color: white;
    border: none;
    border-radius: 8px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
}

.logout-btn:hover {
    background: #dc2626;
    transform: translateY(-1px);
}

.logout-icon {
    font-size: 1rem;
}

.logout-text {
    font-size: 0.875rem;
}

/* Content Area */
.content-area {
    flex: 1;
    padding: 2rem;
    overflow-y: auto;
}

.view-content {
    max-width: 1700px;
    margin: 0 auto;
}

.view-header {
    margin-bottom: 2rem;
    text-align: left;
}

.view-title {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin: 0 0 0.5rem 0;
    font-size: 2rem;
    font-weight: 700;
    color: #1e293b;
}

.view-icon {
    font-size: 2rem;
}

.view-description {
    margin: 0;
    color: #64748b;
    font-size: 1.1rem;
    line-height: 1.5;
}

/* Stats Overview */
.stats-overview {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
}

.stats-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 1.5rem;
}

.stat-card {
    background: white;
    border-radius: 12px;
    padding: 1.5rem;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    border: 1px solid #e5e7eb;
    transition: all 0.2s;
    border-left: 4px solid #3b82f6;
}

.stat-card.hidden {
    visibility: hidden;
}

.stat-card:hover {
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    transform: translateY(-2px);
}

.stat-card.resource {
    border-left: 4px solid #10b981;
}

.stat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
}

.stat-header h3 {
    margin: 0;
    font-size: 1.2rem;
    font-weight: 600;
    color: #6b7280;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.stat-value {
    font-size: 2rem;
    font-weight: 700;
    color: #1f2937;
    margin: 0;
}

.stat-value.large {
    font-size: 2.5rem;
}

.stat-value.small {
    font-size: 1.25rem;
    font-weight: 600;
}

.stat-indicator {
    display: inline-block;
    padding: 0.25rem 0.75rem;
    border-radius: 20px;
    font-size: 0.75rem;
    font-weight: 600;
    margin-top: 0.5rem;
}

.stat-indicator.online {
    background: #d1fae5;
    color: #047857;
}

.stat-indicator.offline {
    background: #fee2e2;
    color: #dc2626;
}

.server-status-indicator {
    display: inline-block;
    padding: 0.25rem 0.75rem;
    border-radius: 20px;
    font-size: 0.75rem;
    font-weight: 600;
    text-align: center;
    align-self: flex-start;
}

.server-status-indicator.online {
    background: #d1fae5;
    color: #047857;
}

.server-status-indicator.offline {
    background: #fee2e2;
    color: #dc2626;
}

.menu-status-indicator {
    display: inline-block;
    padding: 0.125rem 0.5rem;
    border-radius: 12px;
    font-size: 0.625rem;
    font-weight: 600;
    margin-left: 0.5rem;
}

.menu-status-indicator.online {
    background: #d1fae5;
    color: #047857;
}

.menu-status-indicator.offline {
    background: #fee2e2;
    color: #dc2626;
}

.stat-subtitle {
    font-size: 0.875rem;
    font-weight: 500;
    color: #6b7280;
    margin-top: 0.25rem;
}

.stat-progress {
    display: flex;
    align-items: center;
    gap: 1rem;
}

.progress-bar {
    flex: 1;
    height: 8px;
    background: #f3f4f6;
    border-radius: 4px;
    overflow: hidden;
}

.progress-fill {
    height: 100%;
    background: linear-gradient(90deg, #10b981, #059669);
    transition: width 0.3s ease;
    border-radius: 4px;
}

/* Placeholder Content */
.placeholder-content {
    text-align: center;
    padding: 4rem 2rem;
    color: #6b7280;
}

.placeholder-icon {
    font-size: 4rem;
    margin-bottom: 1.5rem;
    opacity: 0.5;
}

.placeholder-content h2 {
    margin: 0 0 1rem 0;
    font-size: 1.5rem;
    color: #374151;
}

.placeholder-content p {
    margin: 0;
    font-size: 1rem;
    max-width: 400px;
    margin-left: auto;
    margin-right: auto;
}

/* Responsive Design */
@media (max-width: 768px) {
    .sidebar {
        width: 100%;
        position: fixed;
        top: 0;
        left: -100%;
        z-index: 1000;
        transition: left 0.3s ease;
    }

    .sidebar.collapsed {
        width: 70px;
        left: -70px;
    }

    .admin-layout.sidebar-open .sidebar {
        left: 0;
    }

    .main-content {
        width: 100%;
    }

    .top-header {
        padding: 1rem;
    }

    .content-area {
        padding: 1rem;
    }

    .stats-row {
        grid-template-columns: 1fr;
    }

    .page-title {
        font-size: 1.25rem;
    }
}
</style>
