export interface LlamaServerStatus {
  running: boolean;
  port: number;
  pid: number | null;
}

export interface GgufModel {
  name: string;
  filename: string;
  path: string;
  size_mb: number;
  is_vlm: boolean;
}

export interface GgufModelList {
  models: GgufModel[];
  models_dir: string;
}

export interface AutoDetect {
  binary_paths: string[];
  binary_path: string;
  mmproj_files: string[];
}
