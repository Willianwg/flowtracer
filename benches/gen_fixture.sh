#!/usr/bin/env bash
# Generate a ~100MB log fixture for performance benchmarking.
# Usage: ./benches/gen_fixture.sh [output_file] [target_mb]
set -euo pipefail

OUTPUT="${1:-benches/big.log}"
TARGET_MB="${2:-100}"
TARGET_BYTES=$((TARGET_MB * 1024 * 1024))

echo "Generating ~${TARGET_MB}MB log fixture → ${OUTPUT}"

LEVELS=("INFO" "DEBUG" "WARN" "ERROR")
FUNCTIONS=("CreateOrderController" "GetUser" "GetCart" "CreateInvoice" "GetProvider"
           "ListProductsController" "GetProducts" "ValidateInput" "ProcessPayment"
           "SendNotification" "UpdateInventory" "AuthMiddleware" "RateLimiter"
           "CacheService" "DatabaseQuery" "ExternalAPICall")
ERRORS=("Connection refused" "Timeout after 30s" "No provider found with name \"paypau\""
        "NullPointerException: Cannot invoke method on null" "Out of memory"
        "Permission denied" "Record not found" "Validation failed: invalid input")

current_bytes=0
req_counter=0
line_counter=0
base_ts=1741770001  # 2025-03-12 10:00:01 UTC approx

{
while [ "$current_bytes" -lt "$TARGET_BYTES" ]; do
    req_counter=$((req_counter + 1))
    req_id=$(printf "req-%06d" "$req_counter")
    num_events=$((RANDOM % 8 + 3))

    for ((e=0; e<num_events; e++)); do
        line_counter=$((line_counter + 1))
        ts_offset=$((line_counter / 10))
        ts=$((base_ts + ts_offset))
        ts_str=$(date -u -d "@$ts" +"%Y-%m-%d %H:%M:%S" 2>/dev/null || date -u -r "$ts" +"%Y-%m-%d %H:%M:%S" 2>/dev/null || echo "2026-03-12 10:10:01")
        ms=$(printf "%03d" $((RANDOM % 1000)))

        if [ "$e" -eq "$((num_events - 1))" ] && [ $((RANDOM % 5)) -eq 0 ]; then
            err_idx=$((RANDOM % ${#ERRORS[@]}))
            line="${ts_str}.${ms} [ERROR] RequestId=${req_id} ${ERRORS[$err_idx]}"
        elif [ "$e" -eq 0 ] || [ $((RANDOM % 3)) -eq 0 ]; then
            fn_idx=$((RANDOM % ${#FUNCTIONS[@]}))
            line="${ts_str}.${ms} [INFO] RequestId=${req_id} Executing ${FUNCTIONS[$fn_idx]}"
        else
            level_idx=$((RANDOM % 2))
            fn_idx=$((RANDOM % ${#FUNCTIONS[@]}))
            line="${ts_str}.${ms} [${LEVELS[$level_idx]}] RequestId=${req_id} ${FUNCTIONS[$fn_idx]} processing step $e"
        fi

        echo "$line"
        current_bytes=$((current_bytes + ${#line} + 1))
    done
done
} > "$OUTPUT"

actual_size=$(wc -c < "$OUTPUT")
actual_mb=$((actual_size / 1024 / 1024))
total_lines=$(wc -l < "$OUTPUT")
echo "Generated: ${actual_mb}MB, ${total_lines} lines, ${req_counter} requests"
