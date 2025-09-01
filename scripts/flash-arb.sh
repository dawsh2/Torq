#!/bin/bash

# Flash Arbitrage System Startup Script (Configuration-based)
# Complete startup sequence for Polygon DEX arbitrage monitoring
# Usage: ./scripts/flash-arb.sh [start|stop|status|restart] [options]

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

# Configuration
CONFIG_FILE="${CONFIG_FILE:-${PROJECT_ROOT}/config/services.toml}"
ENVIRONMENT="${ENVIRONMENT:-development}"
PID_DIR="${PROJECT_ROOT}/.pids"
LOG_DIR="${PROJECT_ROOT}/logs"

# Create necessary directories
mkdir -p "${PID_DIR}" "${LOG_DIR}"

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

# Parse command line options
parse_options() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --config|-c)
                CONFIG_FILE="$2"
                shift 2
                ;;
            --environment|-e)
                ENVIRONMENT="$2"
                shift 2
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                break
                ;;
        esac
    done
}

# Check if a service is running using config-based PID management
is_service_running() {
    local service_name="$1"
    local pid_file="${PID_DIR}/${service_name}.pid"
    
    if [[ -f "$pid_file" ]]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        else
            # Clean up stale PID file
            rm -f "$pid_file"
        fi
    fi
    return 1
}

# Start a service using configuration
start_service() {
    local service_name="$1"
    local binary_name="${2:-$service_name}"
    
    print_step "Starting $service_name..."
    
    if is_service_running "$service_name"; then
        print_warning "$service_name is already running"
        return 0
    fi
    
    cd "${PROJECT_ROOT}"
    
    # Build the service
    cargo build --release --bin "$binary_name" 2>&1 | tee -a "${LOG_DIR}/build_${service_name}.log"
    
    if [[ $? -ne 0 ]]; then
        print_error "Failed to build $service_name"
        return 1
    fi
    
    # Start service with configuration
    RUST_LOG=info cargo run --release --bin "$binary_name" -- \
        --config "${CONFIG_FILE}" \
        --environment "${ENVIRONMENT}" \
        > "${LOG_DIR}/${service_name}.log" 2>&1 &
    
    local pid=$!
    echo "$pid" > "${PID_DIR}/${service_name}.pid"
    
    # Wait a bit for initialization
    sleep 2
    
    if is_service_running "$service_name"; then
        print_success "$service_name started (PID: $pid)"
        return 0
    else
        print_error "$service_name failed to start"
        if [[ -f "${LOG_DIR}/${service_name}.log" ]]; then
            tail -n 10 "${LOG_DIR}/${service_name}.log"
        fi
        return 1
    fi
}

# Stop a service
stop_service() {
    local service_name="$1"
    local pid_file="${PID_DIR}/${service_name}.pid"
    
    if is_service_running "$service_name"; then
        local pid=$(cat "$pid_file")
        print_step "Stopping $service_name (PID: $pid)..."
        
        kill "$pid" 2>/dev/null || true
        
        # Wait for graceful shutdown
        local count=0
        while kill -0 "$pid" 2>/dev/null && [[ $count -lt 10 ]]; do
            sleep 1
            count=$((count + 1))
        done
        
        # Force kill if still running
        if kill -0 "$pid" 2>/dev/null; then
            print_warning "Force killing $service_name..."
            kill -9 "$pid" 2>/dev/null || true
        fi
        
        rm -f "$pid_file"
        print_success "$service_name stopped"
    else
        print_info "$service_name not running"
    fi
}

# Get list of active services from configuration
get_active_services() {
    # For now, return default services
    # TODO: Parse from configuration file
    if [[ "${DEPLOYMENT_MODE:-separate}" == "enriched" ]]; then
        echo "start_market_data_relay start_signal_relay polygon_enriched flash_arbitrage torq-dashboard-websocket"
    else
        echo "start_market_data_relay start_signal_relay polygon_event_collector polygon_pool_metadata flash_arbitrage torq-dashboard-websocket"
    fi
}

# Start all services
start_all() {
    print_info "${BOLD}Starting Flash Arbitrage System...${NC}"
    print_info "📁 Config: ${CONFIG_FILE}"
    print_info "🌍 Environment: ${ENVIRONMENT}"
    echo ""
    
    # Validate configuration
    if [[ ! -f "${CONFIG_FILE}" ]]; then
        print_error "Configuration file not found: ${CONFIG_FILE}"
        return 1
    fi
    
    # Start services in order
    local services
    services=$(get_active_services)
    
    for service in $services; do
        start_service "$service" || {
            print_error "Failed to start $service"
            return 1
        }
    done
    
    echo ""
    print_success "${BOLD}Flash Arbitrage System Started Successfully!${NC}"
    echo ""
    print_info "📊 Dashboard: http://localhost:8766"
    print_info "📝 Logs: ${LOG_DIR}/"
    print_info "🔍 Monitor: tail -f ${LOG_DIR}/*.log"
    echo ""
}

# Stop all services
stop_all() {
    print_info "${BOLD}Stopping Flash Arbitrage System...${NC}"
    echo ""
    
    # Stop in reverse order
    local services
    services=$(get_active_services)
    
    for service in $(echo $services | tr ' ' '\n' | tac); do
        stop_service "$service"
    done
    
    # Clean up sockets
    rm -f /tmp/torq/*.sock 2>/dev/null || true
    
    echo ""
    print_success "${BOLD}Flash Arbitrage System Stopped${NC}"
    echo ""
}

# Show status of all services
show_status() {
    print_info "${BOLD}Flash Arbitrage System Status${NC}"
    print_info "📁 Config: ${CONFIG_FILE}"
    print_info "🌍 Environment: ${ENVIRONMENT}"
    echo ""
    
    local services
    services=$(get_active_services)
    
    for service in $services; do
        if is_service_running "$service"; then
            local pid=$(cat "${PID_DIR}/${service}.pid")
            print_success "$service: Running (PID: $pid)"
        else
            print_error "$service: Not running"
        fi
    done
    
    echo ""
}

# Show help
show_help() {
    echo "Flash Arbitrage System Manager (Configuration-based)"
    echo "===================================================="
    echo ""
    echo "Usage: $0 [command] [options]"
    echo ""
    echo "Commands:"
    echo "  start    Start all services"
    echo "  stop     Stop all services"
    echo "  restart  Restart all services"
    echo "  status   Show service status"
    echo "  logs     Follow all service logs"
    echo "  help     Show this help message"
    echo ""
    echo "Options:"
    echo "  -c, --config FILE      Configuration file (default: config/services.toml)"
    echo "  -e, --environment ENV  Environment name (default: development)"
    echo "  -h, --help            Show this help"
    echo ""
    echo "Examples:"
    echo "  $0 start                                    # Start with defaults"
    echo "  $0 start -e production                     # Start in production mode"
    echo "  $0 start -c config/custom.toml -e staging  # Custom config and environment"
    echo ""
    echo "Environment Variables:"
    echo "  CONFIG_FILE    - Default configuration file"
    echo "  ENVIRONMENT    - Default environment"
    echo ""
}

# Main command dispatcher
COMMAND="${1:-help}"
shift 2>/dev/null || true

# Parse remaining options
parse_options "$@"

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
        show_help
        ;;
    
    *)
        print_error "Unknown command: $COMMAND"
        echo "Use '$0 help' for usage information"
        exit 1
        ;;
esac