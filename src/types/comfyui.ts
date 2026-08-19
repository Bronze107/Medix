export interface ComfyWorkflow {
  id: string;
  name: string;
  workflow_type: "generate" | "edit";
  workflow_json: string;
  created_at: string;
  updated_at: string;
}

export type WorkflowParamType =
  | "text"
  | "multiline"
  | "number"
  | "slider"
  | "seed"
  | "combo"
  | "image_selector"
  | "boolean";

export interface WorkflowParam {
  node_id: string;
  widget_name: string;
  /** 稳定键: "{node_id}:{widget_name}"，表单 values 以此为 key */
  param_name: string;
  /** 表单显示标签 */
  label: string;
  default_value: string;
  field_type: WorkflowParamType;
  order_index: number;
  min?: number | null;
  max?: number | null;
  step?: number | null;
  /** combo 枚举选项 */
  options?: string[];
  /** STRING 多行输入 */
  multiline?: boolean;
  description?: string | null;
}

export interface ComfyWorkflowDetail extends ComfyWorkflow {
  params: WorkflowParam[];
  result_nodes: string[];
}
