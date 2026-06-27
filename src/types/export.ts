export interface ExportOptions {
  media_ids: string[];
  caption_mode: "all" | "manual" | "ai" | "latest";
  export_original: boolean;
  export_json: boolean;
  variant_presets: string[];
  /** If set, only export these specific variant IDs (instead of all variants). */
  variant_ids?: string[];
  output_dir: string;
  use_zip: boolean;
}

export interface ExportProgress {
  current: number;
  total: number;
  filename: string;
}
