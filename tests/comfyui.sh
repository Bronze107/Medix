#!/bin/bash
set -euo pipefail
source "$(dirname "$0")/_helpers.sh"

echo "=== ComfyUI Workflow CRUD Tests ==="

setup_isolated_db "comfyui-test"

# Create workflows via direct SQL
exec_sql "INSERT INTO comfyui_workflows (id, name, workflow_type, workflow_json, created_at, updated_at)
  VALUES ('01TEST', 'Test Generate', 'generate', '{\"nodes\":[{\"id\":\"1\",\"type\":\"CLIPTextEncode\",\"title\":\"#prompt\",\"widgets_values\":[\"\"]}]}', datetime('now'), datetime('now'))"

exec_sql "INSERT INTO comfyui_workflows (id, name, workflow_type, workflow_json, created_at, updated_at)
  VALUES ('02TEST', 'Test Edit', 'edit', '{\"nodes\":[{\"id\":\"1\",\"type\":\"LoadImage\",\"title\":\"#input_image\",\"widgets_values\":[\"\"]},{\"id\":\"2\",\"type\":\"CLIPTextEncode\",\"title\":\"#prompt\",\"widgets_values\":[\"\"]}]}', datetime('now'), datetime('now'))"

# List all
count=$(q "SELECT COUNT(*) FROM comfyui_workflows")
check "list all workflows" "2" "$count"

# Filter by type
gen_count=$(q "SELECT COUNT(*) FROM comfyui_workflows WHERE workflow_type='generate'")
check "filter generate type" "1" "$gen_count"

edit_count=$(q "SELECT COUNT(*) FROM comfyui_workflows WHERE workflow_type='edit'")
check "filter edit type" "1" "$edit_count"

# Update
exec_sql "UPDATE comfyui_workflows SET name='Updated Name' WHERE id='01TEST'"
updated=$(q "SELECT name FROM comfyui_workflows WHERE id='01TEST'")
check "update workflow name" "Updated Name" "$updated"

# Delete
exec_sql "DELETE FROM comfyui_workflows WHERE id='02TEST'"
remaining=$(q "SELECT COUNT(*) FROM comfyui_workflows")
check "delete workflow" "1" "$remaining"

# Settings
exec_sql "INSERT INTO settings (key, value) VALUES ('comfyui_base_url', 'http://localhost:8188')"
base_url=$(q "SELECT value FROM settings WHERE key='comfyui_base_url'")
check "comfyui base url setting" "http://localhost:8188" "$base_url"

# Verify migration 0024 exists
mig=$(q "SELECT COUNT(*) FROM _migrations WHERE name='0024_comfyui_workflows'")
check "migration 0024 applied" "1" "$mig"

final_report
