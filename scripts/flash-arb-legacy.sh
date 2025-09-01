#!/bin/bash

# Flash Arbitrage System Startup Script
# Complete startup sequence for Polygon DEX arbitrage monitoring
# Usage: ./scripts/flash-arb.sh [start|stop|status|restart]

set -euo pipefail

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Get the script's directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "${SCRIPT_DIR}/.." && pwd )"

# Socket directory
SOCKET_DIR="/tmp/torq"
PID_DIR="${PROJECT_ROOT}/.pids"
LOG_DIR="${PROJECT_ROOT}/logs"

# Create necessary directories
mkdir -p "${SOCKET_DIR}"
mkdir -p "${PID_DIR}"
mkdir -p "${LOG_DIR}"

# Service PIDs storage
MARKET_DATA_RELAY_PID_FILE="${PID_DIR}/market_data_relay.pid"
SIGNAL_RELAY_PID_FILE="${PID_DIR}/signal_relay.pid"
POLYGON_EVENT_COLLECTOR_PID_FILE="${PID_DIR}/polygon_event_collector.pid"
POLYGON_POOL_METADATA_PID_FILE="${PID_DIR}/polygon_pool_metadata.pid"
FLASH_ARB_PID_FILE="${PID_DIR}/flash_arbitrage.pid"
DASHBOARD_PID_FILE="${PID_DIR}/dashboard.pid"

# Configuration
USE_ENRICHED_ADAPTER="${USE_ENRICHED_ADAPTER:-false}"  # Set to true to use combined adapter
POLYGON_RAW_SOCKET="${SOCKET_DIR}/polygon_raw.sock"  # Direct connection for enrichment

# Functions for colored output
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[⚠]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1" >&2
}

print_step() {
    echo -e "${BOLD}→${NC} $1"
}

# Check if a service is running
is_running() {
    local pid_file="$1"
    if [[ -f "$pid_file" ]]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
    fi
    return 1
}

# Stop a service
stop_service() {
    local name="$1"
    local pid_file="$2"
    
    if is_running "$pid_file"; then
        local pid=$(cat "$pid_file")
        print_step "Stopping $name (PID: $pid)..."
        kill "$pid" 2>/dev/null || true
        
        # Wait for graceful shutdown
        local count=0
        while kill -0 "$pid" 2>/dev/null && [[ $count -lt 10 ]]; do
            sleep 1
            count=$((count + 1))
        done
        
        # Force kill if still running
        if kill -0 "$pid" 2>/dev/null; then
            print_warning "Force killing $name..."
            kill -9 "$pid" 2>/dev/null || true
        fi
        
        rm -f "$pid_file"
        print_success "$name stopped"
    else
        print_info "$name not running"
    fi
}

# Start the market data relay
start_market_data_relay() {
    print_step "Starting Market Data Relay..."
    
    # Remove old socket if exists
    rm -f "${SOCKET_DIR}/market_data.sock"
    
    # Build and start the relay
    cd "${PROJECT_ROOT}"
    cargo build --release --bin start_market_data_relay 2>&1 | tee -a "${LOG_DIR}/build_relay.log"
    
    RUST_LOG=info cargo run --release --bin start_market_data_relay \
        > "${LOG_DIR}/market_data_relay.log" 2>&1 &
    
    local pid=$!
    echo "$pid" > "${MARKET_DATA_RELAY_PID_FILE}"
    
    # Wait for socket to be created
    local count=0
    while [[ ! -S "${SOCKET_DIR}/market_data.sock" ]] && [[ $count -lt 10 ]]; do
        sleep 1
        count=$((count + 1))
    done
    
    if [[ -S "${SOCKET_DIR}/market_data.sock" ]]; then
        print_success "Market Data Relay started (PID: $pid)"
        return 0
    else
        print_error "Market Data Relay failed to create socket"
        return 1
    fi
}

# Start the signal relay
start_signal_relay() {
    print_step "Starting Signal Relay..."
    
    # Remove old socket if exists
    rm -f "${SOCKET_DIR}/signals.sock"
    
    # Build and start the relay
    cd "${PROJECT_ROOT}"
    cargo build --release --bin start_signal_relay 2>&1 | tee -a "${LOG_DIR}/build_relay.log"
    
    RUST_LOG=info cargo run --release --bin start_signal_relay \
        > "${LOG_DIR}/signal_relay.log" 2>&1 &
    
    local pid=$!
    echo "$pid" > "${SIGNAL_RELAY_PID_FILE}"
    
    # Wait for socket to be created
    local count=0
    while [[ ! -S "${SOCKET_DIR}/signals.sock" ]] && [[ $count -lt 10 ]]; do
        sleep 1
        count=$((count + 1))
    done
    
    if [[ -S "${SOCKET_DIR}/signals.sock" ]]; then
        print_success "Signal Relay started (PID: $pid)"
        return 0
    else
        print_error "Signal Relay failed to create socket"
        return 1
    fi
}

# Start the Polygon Pool Metadata Service (enrichment middleware)
start_polygon_pool_metadata() {
    print_step "Starting Polygon Pool Metadata Service..."
    
    cd "${PROJECT_ROOT}"
    
    # Ensure cache directory exists
    mkdir -p "${PROJECT_ROOT}/data/polygon_pool_cache"
    
    # Build the service
    cargo build --release --bin polygon_pool_metadata 2>&1 | tee -a "${LOG_DIR}/build_polygon_pool_metadata.log"
    
    # Start with proper configuration
    RUST_LOG=polygon_pool_metadata=info,pool_metadata_adapter=info \
    POLYGON_RPC_URL="${POLYGON_RPC_URL:-https://polygon-rpc.com}" \
    POLYGON_RAW_SOCKET="${POLYGON_RAW_SOCKET}" \
    MARKET_DATA_SOCKET="${SOCKET_DIR}/market_data.sock" \
        cargo run --release --bin polygon_pool_metadata \
        > "${LOG_DIR}/polygon_pool_metadata.log" 2>&1 &
    
    local pid=$!
    echo "$pid" > "${POLYGON_POOL_METADATA_PID_FILE}"
    
    # Wait a bit for initialization
    sleep 2
    
    if is_running "${POLYGON_POOL_METADATA_PID_FILE}"; then
        print_success "Polygon Pool Metadata Service started (PID: $pid)"
        
        # Check cache status
        if [[ -f "${PROJECT_ROOT}/data/polygon_pool_cache/pool_metadata.json" ]]; then
            local pool_count=$(jq '. | length' "${PROJECT_ROOT}/data/polygon_pool_cache/pool_metadata.json" 2>/dev/null || echo "0")
            print_success "Polygon pool cache loaded with $pool_count pools"
        fi
        return 0
    else
        print_error "Polygon Pool Metadata Service failed to start"
        tail -n 20 "${LOG_DIR}/polygon_pool_metadata.log"
        return 1
    fi
}

# Start the Polygon Event Collector
start_polygon_event_collector() {
    if [[ "${USE_ENRICHED_ADAPTER}" == "true" ]]; then
        print_step "Starting Polygon Enriched Adapter (combined mode)..."
        
        cd "${PROJECT_ROOT}"
        
        # Build the enriched adapter
        cargo build --release --bin polygon_enriched 2>&1 | tee -a "${LOG_DIR}/build_polygon_enriched.log"
        
        # Start enriched adapter (includes pool metadata functionality)
        # Writes directly to market data relay
        RUST_LOG=polygon_enriched=debug,polygon_adapter=debug,pool_metadata_adapter=debug \
        POLYGON_RPC_URL="${POLYGON_RPC_URL:-https://polygon-rpc.com}" \
            cargo run --release --bin polygon_enriched \
            > "${LOG_DIR}/polygon_enriched.log" 2>&1 &
        
        local pid=$!
        echo "$pid" > "${POLYGON_EVENT_COLLECTOR_PID_FILE}"
        echo "$pid" > "${POLYGON_POOL_METADATA_PID_FILE}"  # Combined service
        
        sleep 3
        
        if is_running "${POLYGON_EVENT_COLLECTOR_PID_FILE}"; then
            print_success "Polygon Enriched Adapter started (PID: $pid)"
            print_success "Pool metadata enrichment integrated"
            return 0
        else
            print_error "Polygon Enriched Adapter failed to start"
            tail -n 20 "${LOG_DIR}/polygon_enriched.log"
            return 1
        fi
    else
        print_step "Starting Polygon Event Collector (separate mode)..."
        
        cd "${PROJECT_ROOT}"
        
        # Remove old raw socket if exists
        rm -f "${POLYGON_RAW_SOCKET}"
        
        # Build the event collector
        cargo build --release --bin polygon_event_collector 2>&1 | tee -a "${LOG_DIR}/build_polygon_event_collector.log"
        
        # Start event collector - outputs to raw socket for enrichment
        RUST_LOG=polygon_event_collector=debug,adapters=debug,codec=debug \
        OUTPUT_SOCKET="${POLYGON_RAW_SOCKET}" \
            cargo run --release --bin polygon_event_collector \
            > "${LOG_DIR}/polygon_event_collector.log" 2>&1 &
        
        local pid=$!
        echo "$pid" > "${POLYGON_EVENT_COLLECTOR_PID_FILE}"
        
        # Wait for socket creation
        local count=0
        while [[ ! -S "${POLYGON_RAW_SOCKET}" ]] && [[ $count -lt 10 ]]; do
            sleep 1
            count=$((count + 1))
        done
        
        if [[ -S "${POLYGON_RAW_SOCKET}" ]]; then
            print_success "Polygon Event Collector started (PID: $pid)"
            print_success "Raw events socket created: ${POLYGON_RAW_SOCKET}"
            return 0
        else
            print_error "Polygon Event Collector failed to create socket"
            tail -n 20 "${LOG_DIR}/polygon_event_collector.log"
            return 1
        fi
    fi
}

# Start the flash arbitrage strategy
start_flash_arbitrage() {
    print_step "Starting Flash Arbitrage Strategy..."
    
    cd "${PROJECT_ROOT}"
    
    # Build the strategy
    cargo build --release --bin flash_arbitrage 2>&1 | tee -a "${LOG_DIR}/build_flash_arb.log"
    
    # Start with reduced logging to prevent disk filling
    # Use warn level and redirect to /dev/null to prevent log file growth
    RUST_LOG=flash_arbitrage=warn,strategies=warn \
        cargo run --release --bin flash_arbitrage \
        > /dev/null 2>&1 &
    
    local pid=$!
    echo "$pid" > "${FLASH_ARB_PID_FILE}"
    
    sleep 2
    
    if is_running "${FLASH_ARB_PID_FILE}"; then
        print_success "Flash Arbitrage Strategy started (PID: $pid)"
        return 0
    else
        print_error "Flash Arbitrage Strategy failed to start"
        # Log file no longer created to prevent disk filling
        return 1
    fi
}

# Start the dashboard
start_dashboard() {
    print_step "Starting Dashboard WebSocket Service..."
    
    cd "${PROJECT_ROOT}"
    
    # Build the dashboard
    cargo build --release --bin torq-dashboard-websocket 2>&1 | tee -a "${LOG_DIR}/build_dashboard.log"
    
    # Start dashboard with proper relay connections
    RUST_LOG=dashboard=debug,observability=debug \
        cargo run --release --bin torq-dashboard-websocket -- \
        --port 8766 \
        --market-data-relay "${SOCKET_DIR}/market_data.sock" \
        --signal-relay "${SOCKET_DIR}/signals.sock" \
        > "${LOG_DIR}/dashboard.log" 2>&1 &
    
    local pid=$!
    echo "$pid" > "${DASHBOARD_PID_FILE}"
    
    sleep 2
    
    if is_running "${DASHBOARD_PID_FILE}"; then
        print_success "Dashboard started (PID: $pid) - WebSocket available at ws://localhost:8766"
        return 0
    else
        print_error "Dashboard failed to start"
        tail -n 20 "${LOG_DIR}/dashboard.log"
        return 1
    fi
}

# Start all services
start_all() {
    print_info "${BOLD}Starting Flash Arbitrage System...${NC}"
    echo ""
    
    # Start relays first (they need to be ready for connections)
    start_market_data_relay || {
        print_error "Failed to start Market Data Relay"
        return 1
    }
    
    start_signal_relay || {
        print_error "Failed to start Signal Relay"
        return 1
    }
    
    # Give relays time to fully initialize
    sleep 2
    
    # Start data producers
    if [[ "${USE_ENRICHED_ADAPTER}" == "true" ]]; then
        # Combined mode: single service handles both collection and enrichment
        start_polygon_event_collector || {
            print_error "Failed to start Polygon Enriched Adapter"
            return 1
        }
    else
        # Separate mode: event collector → pool metadata → market data relay
        start_polygon_event_collector || {
            print_error "Failed to start Polygon Event Collector"
            return 1
        }
        
        # Give event collector time to create socket
        sleep 1
        
        start_polygon_pool_metadata || {
            print_error "Failed to start Polygon Pool Metadata Service"
            return 1
        }
    fi
    
    # Start consumers
    start_flash_arbitrage || {
        print_error "Failed to start Flash Arbitrage Strategy"
        return 1
    }
    
    start_dashboard || {
        print_error "Failed to start Dashboard"
        return 1
    }
    
    echo ""
    print_success "${BOLD}Flash Arbitrage System Started Successfully!${NC}"
    echo ""
    print_info "Dashboard: http://localhost:8766"
    print_info "Logs: ${LOG_DIR}/"
    print_info "Monitor with: tail -f ${LOG_DIR}/*.log"
    echo ""
}

# Stop all services
stop_all() {
    print_info "${BOLD}Stopping Flash Arbitrage System...${NC}"
    echo ""
    
    # Stop in reverse order
    stop_service "Dashboard" "${DASHBOARD_PID_FILE}"
    stop_service "Flash Arbitrage" "${FLASH_ARB_PID_FILE}"
    
    if [[ "${USE_ENRICHED_ADAPTER}" == "true" ]]; then
        # Combined service
        stop_service "Polygon Enriched Adapter" "${POLYGON_EVENT_COLLECTOR_PID_FILE}"
    else
        # Separate services
        stop_service "Polygon Pool Metadata" "${POLYGON_POOL_METADATA_PID_FILE}"
        stop_service "Polygon Event Collector" "${POLYGON_EVENT_COLLECTOR_PID_FILE}"
    fi
    
    stop_service "Signal Relay" "${SIGNAL_RELAY_PID_FILE}"
    stop_service "Market Data Relay" "${MARKET_DATA_RELAY_PID_FILE}"
    
    # Clean up sockets
    rm -f "${SOCKET_DIR}/market_data.sock"
    rm -f "${SOCKET_DIR}/signals.sock"
    rm -f "${SOCKET_DIR}/execution.sock"
    rm -f "${POLYGON_RAW_SOCKET}"
    
    echo ""
    print_success "${BOLD}Flash Arbitrage System Stopped${NC}"
    echo ""
}

# Show status of all services
show_status() {
    print_info "${BOLD}Flash Arbitrage System Status${NC}"
    echo ""
    
    # Check each service
    if [[ "${USE_ENRICHED_ADAPTER}" == "true" ]]; then
        services=(
            "Market Data Relay:${MARKET_DATA_RELAY_PID_FILE}"
            "Signal Relay:${SIGNAL_RELAY_PID_FILE}"
            "Polygon Enriched:${POLYGON_EVENT_COLLECTOR_PID_FILE}"
            "Flash Arbitrage:${FLASH_ARB_PID_FILE}"
            "Dashboard:${DASHBOARD_PID_FILE}"
        )
    else
        services=(
            "Market Data Relay:${MARKET_DATA_RELAY_PID_FILE}"
            "Signal Relay:${SIGNAL_RELAY_PID_FILE}"
            "Polygon Event Collector:${POLYGON_EVENT_COLLECTOR_PID_FILE}"
            "Polygon Pool Metadata:${POLYGON_POOL_METADATA_PID_FILE}"
            "Flash Arbitrage:${FLASH_ARB_PID_FILE}"
            "Dashboard:${DASHBOARD_PID_FILE}"
        )
    fi
    
    for service in "${services[@]}"; do
        
        IFS=':' read -r name pid_file <<< "$service"
        
        if is_running "$pid_file"; then
            local pid=$(cat "$pid_file")
            print_success "$name: Running (PID: $pid)"
        else
            print_error "$name: Not running"
        fi
    done
    
    echo ""
    
    # Check sockets
    print_info "Unix Sockets:"
    for socket in "${SOCKET_DIR}/market_data.sock" \
                  "${SOCKET_DIR}/signals.sock" \
                  "${SOCKET_DIR}/execution.sock"; do
        if [[ -S "$socket" ]]; then
            print_success "  $socket: Active"
        else
            print_warning "  $socket: Not found"
        fi
    done
    
    echo ""
    
    # Show recent log activity
    print_info "Recent Activity:"
    for log in "${LOG_DIR}/polygon_adapter.log" \
               "${LOG_DIR}/market_data_relay.log"; do
        if [[ -f "$log" ]]; then
            local name=$(basename "$log" .log)
            local last_line=$(tail -n 1 "$log" 2>/dev/null | head -c 80)
            echo "  $name: $last_line..."
        fi
    done
    
    echo ""
}

# Main command dispatcher
COMMAND="${1:-help}"

case "$COMMAND" in
    start|up)
        start_all
        ;;
    
    stop|down)
        stop_all
        ;;
    
    restart)
        stop_all
        sleep 2
        start_all
        ;;
    
    status)
        show_status
        ;;
    
    logs)
        print_info "Following all logs..."
        tail -f "${LOG_DIR}"/*.log
        ;;
    
    help|--help|-h)
        echo "Flash Arbitrage System Manager"
        echo "=============================="
        echo ""
        echo "Usage: $0 [command]"
        echo ""
        echo "Commands:"
        echo "  start    Start all flash arbitrage services"
        echo "  stop     Stop all services"
        echo "  restart  Restart all services"
        echo "  status   Show service status"
        echo "  logs     Follow all service logs"
        echo "  help     Show this help message"
        echo ""
        echo "Services Architecture:"
        echo ""
        echo "SEPARATE MODE (default):"
        echo "  1. Market Data Relay (Unix socket relay for enriched events)"
        echo "  2. Signal Relay (Unix socket relay for arbitrage signals)"
        echo "  3. Polygon Event Collector (WebSocket → raw events)"
        echo "  4. Polygon Pool Metadata (enriches events with decimals)"
        echo "  5. Flash Arbitrage Strategy (consumes enriched events)"
        echo "  6. Dashboard (WebSocket monitoring)"
        echo ""
        echo "  Data flow: Polygon WS → Event Collector → Pool Metadata → Market Relay → Strategy"
        echo ""
        echo "COMBINED MODE (USE_ENRICHED_ADAPTER=true):"
        echo "  1. Market Data Relay (Unix socket relay)"
        echo "  2. Signal Relay (Unix socket relay)"
        echo "  3. Polygon Enriched Adapter (collection + enrichment in one)"
        echo "  4. Flash Arbitrage Strategy (arbitrage detector)"
        echo "  5. Dashboard (WebSocket monitoring)"
        echo ""
        echo "Environment Variables:"
        echo "  USE_ENRICHED_ADAPTER  - true for combined mode, false for separate (default: false)"
        echo "  POLYGON_RPC_URL       - RPC endpoint for pool discovery (default: https://polygon-rpc.com)"
        echo ""
        ;;
    
    *)
        print_error "Unknown command: $COMMAND"
        echo "Use '$0 help' for usage information"
        exit 1
        ;;
esac