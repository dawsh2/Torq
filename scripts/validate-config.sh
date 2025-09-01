#!/bin/bash

# Configuration Validation Script
# Validates Torq configuration files and checks connectivity

set -euo pipefail

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "${SCRIPT_DIR}/.." && pwd )"

CONFIG_FILE="${1:-${PROJECT_ROOT}/config/services.toml}"
ENVIRONMENT="${2:-development}"

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
    echo -e "${RED}[✗]${NC} $1"
}

print_header() {
    echo -e "${BOLD}$1${NC}"
    echo "$(printf '=%.0s' $(seq 1 ${#1}))"
    echo
}

# Check if required tools are available
check_dependencies() {
    print_header "Checking Dependencies"
    
    local missing=0
    
    for tool in cargo toml jq nc; do
        if command -v "$tool" >/dev/null 2>&1; then
            print_success "$tool: Available"
        else
            print_error "$tool: Missing"
            missing=$((missing + 1))
        fi
    done
    
    if [[ $missing -gt 0 ]]; then
        print_error "$missing dependencies missing"
        return 1
    fi
    
    echo
}

# Validate configuration file syntax
validate_config_syntax() {
    print_header "Validating Configuration Syntax"
    
    if [[ ! -f "$CONFIG_FILE" ]]; then
        print_error "Configuration file not found: $CONFIG_FILE"
        return 1
    fi
    
    # Check TOML syntax
    if toml --help >/dev/null 2>&1 && toml check "$CONFIG_FILE" >/dev/null 2>&1; then
        print_success "TOML syntax: Valid"
    else
        print_warning "TOML validation tool not available or config has issues"
    fi
    
    # Check if environment config exists
    local env_config="${PROJECT_ROOT}/config/environments/${ENVIRONMENT}.toml"
    if [[ -f "$env_config" ]]; then
        print_success "Environment config: Found (${ENVIRONMENT}.toml)"
        if toml check "$env_config" >/dev/null 2>&1; then
            print_success "Environment TOML: Valid"
        else
            print_warning "Environment TOML may have syntax issues"
        fi
    else
        print_warning "Environment config not found: $env_config"
    fi
    
    echo
}

# Check service definitions
validate_services() {
    print_header "Validating Service Definitions"
    
    local services=(
        "market_data_relay"
        "signal_relay" 
        "polygon_event_collector"
        "polygon_pool_metadata"
        "polygon_enriched"
        "flash_arbitrage"
        "dashboard"
    )
    
    for service in "${services[@]}"; do
        # This would ideally parse the TOML, but for now just check basic structure
        if grep -q "\\[services\\.${service}\\]" "$CONFIG_FILE"; then
            print_success "Service definition: $service"
        else
            print_warning "Service definition missing: $service"
        fi
    done
    
    echo
}

# Check socket directories and permissions
check_socket_permissions() {
    print_header "Checking Socket Directories"
    
    local socket_dirs=("/tmp/torq" "/tmp/torq_dev" "/var/run/torq")
    
    for dir in "${socket_dirs[@]}"; do
        if [[ -d "$dir" ]]; then
            if [[ -w "$dir" ]]; then
                print_success "Socket directory: $dir (writable)"
            else
                print_warning "Socket directory: $dir (not writable)"
            fi
        else
            print_info "Socket directory: $dir (will be created)"
        fi
    done
    
    echo
}

# Test RPC connectivity
test_rpc_connectivity() {
    print_header "Testing RPC Connectivity"
    
    local rpcs=(
        "https://polygon-rpc.com"
        "https://rpc-mainnet.matic.network"
        "https://arb1.arbitrum.io/rpc"
        "https://mainnet.base.org"
    )
    
    for rpc in "${rpcs[@]}"; do
        print_info "Testing: $rpc"
        
        # Simple HTTP connectivity test
        if curl -s --max-time 5 "$rpc" >/dev/null 2>&1; then
            print_success "  Reachable"
        else
            # Test with JSON-RPC call
            if curl -s --max-time 5 -X POST \
                -H "Content-Type: application/json" \
                -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' \
                "$rpc" >/dev/null 2>&1; then
                print_success "  RPC responding"
            else
                print_warning "  Unreachable or not responding"
            fi
        fi
    done
    
    echo
}

# Check cache directories
check_cache_directories() {
    print_header "Checking Cache Directories"
    
    local cache_dirs=(
        "./data/polygon_pool_cache"
        "./data/arbitrum_pool_cache" 
        "./data/base_pool_cache"
        "/var/lib/torq"
    )
    
    for dir in "${cache_dirs[@]}"; do
        local full_path="${PROJECT_ROOT}/${dir}"
        if [[ "${dir:0:1}" == "/" ]]; then
            full_path="$dir"
        fi
        
        if [[ -d "$full_path" ]]; then
            local files=$(find "$full_path" -name "*.json" -o -name "*.tlv" 2>/dev/null | wc -l)
            print_success "Cache directory: $dir ($files files)"
        else
            print_info "Cache directory: $dir (will be created)"
        fi
    done
    
    echo
}

# Check binary dependencies
check_binaries() {
    print_header "Checking Binary Dependencies"
    
    cd "$PROJECT_ROOT"
    
    local binaries=(
        "start_market_data_relay"
        "start_signal_relay"
        "polygon_event_collector"
        "polygon_pool_metadata"
        "polygon_enriched"
        "flash_arbitrage"
        "torq-dashboard-websocket"
    )
    
    for binary in "${binaries[@]}"; do
        if cargo build --release --bin "$binary" >/dev/null 2>&1; then
            print_success "Binary builds: $binary"
        else
            print_error "Binary fails to build: $binary"
        fi
    done
    
    echo
}

# Generate configuration summary
generate_summary() {
    print_header "Configuration Summary"
    
    print_info "Configuration file: $CONFIG_FILE"
    print_info "Environment: $ENVIRONMENT"
    print_info "Project root: $PROJECT_ROOT"
    print_info "Socket directory: /tmp/torq"
    print_info "Log directory: ${PROJECT_ROOT}/logs"
    print_info "PID directory: ${PROJECT_ROOT}/.pids"
    
    echo
    print_info "To start the system:"
    print_info "  ./scripts/flash-arb.sh start -e $ENVIRONMENT"
    echo
    print_info "To monitor logs:"
    print_info "  ./scripts/flash-arb.sh logs"
    echo
}

# Main validation flow
main() {
    echo
    print_header "Torq Configuration Validator"
    echo "Config: $CONFIG_FILE"
    echo "Environment: $ENVIRONMENT"
    echo
    
    check_dependencies || {
        print_error "Dependencies check failed"
        exit 1
    }
    
    validate_config_syntax || {
        print_error "Configuration validation failed"
        exit 1
    }
    
    validate_services
    check_socket_permissions
    check_cache_directories
    
    if [[ "${SKIP_NETWORK_TESTS:-false}" != "true" ]]; then
        test_rpc_connectivity
    else
        print_info "Skipping network connectivity tests (SKIP_NETWORK_TESTS=true)"
    fi
    
    if [[ "${SKIP_BUILD_TESTS:-false}" != "true" ]]; then
        check_binaries
    else
        print_info "Skipping build tests (SKIP_BUILD_TESTS=true)"
        echo
    fi
    
    generate_summary
    
    print_success "Configuration validation completed!"
}

# Show help
show_help() {
    echo "Torq Configuration Validator"
    echo ""
    echo "Usage: $0 [config_file] [environment]"
    echo ""
    echo "Arguments:"
    echo "  config_file   Configuration file to validate (default: config/services.toml)"
    echo "  environment   Environment name (default: development)"
    echo ""
    echo "Environment Variables:"
    echo "  SKIP_NETWORK_TESTS=true   Skip RPC connectivity tests"
    echo "  SKIP_BUILD_TESTS=true     Skip binary build tests"
    echo ""
    echo "Examples:"
    echo "  $0                                      # Validate default config"
    echo "  $0 config/services.toml production      # Validate production config"
    echo "  SKIP_NETWORK_TESTS=true $0              # Skip network tests"
}

# Handle help flag
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    show_help
    exit 0
fi

# Run main validation
main