export interface OllamaModel {
  name: string;
  size: number;
  digest: string;
}

export interface OllamaStatus {
  running: boolean;
  version: string | null;
  models: OllamaModel[];
}

export interface ModelStatus {
  name: string;
  installed: boolean;
  size_mb: number;
}
