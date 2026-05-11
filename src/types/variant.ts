export interface Variant {
  id: string;
  media_id: string;
  preset_name: string;
  format: string;
  width: number | null;
  height: number | null;
  quality: number | null;
  file_size: number | null;
  file_path: string;
}

export interface VariantPreset {
  name: string;
  label: string;
  format: string;
  max_width: number | null;
  max_height: number | null;
  quality: number;
}
