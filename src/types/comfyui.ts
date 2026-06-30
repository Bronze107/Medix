export interface ComfyWorkflow {
  id: string;
  name: string;
  workflow_type: "generate" | "edit";
  workflow_json: string;
  created_at: string;
  updated_at: string;
}

export interface WorkflowParam {
  node_id: string;
  param_name: string;
  widget_name: string;
  default_value: string;
  field_type: "text" | "multiline" | "number" | "slider" | "seed" | "image_selector";
  order_index: number;
}

export interface ComfyWorkflowDetail extends ComfyWorkflow {
  params: WorkflowParam[];
}
