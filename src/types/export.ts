export interface ExportOptions {
  media_ids: string[];
  caption_mode: "all" | "manual" | "ai";
  export_original: boolean;
  variant_presets: string[];
  output_dir: string;
  use_zip: boolean;
}

export interface ExportProgress {
  current: number;
  total: number;
  filename: string;
}
