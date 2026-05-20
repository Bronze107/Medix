import { useCallback, useEffect, useState } from "react";
import type { LlamaServerStatus, GgufModelList, AutoDetect } from "@/types/ai";
import {
  autoDetect,
  llamaServerStatus,
  llamaServerStart,
  llamaServerStop,
  modelList,
  settingsGetAll,
  settingsSet,
} from "@/lib/tauri";

type AiMode = "local" | "cloud" | "auto";
type CloudProvider = "claude" | "openai" | "qwen";

function Settings() {
  const [serverStatus, setServerStatus] = useState<LlamaServerStatus | null>(null);
  const [ggufList, setGgufList] = useState<GgufModelList | null>(null);
  const [loading, setLoading] = useState(true);
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);

  const [aiMode, setAiMode] = useState<AiMode>("auto");
  const [cloudProvider, setCloudProvider] = useState<CloudProvider>("claude");
  const [apiKey, setApiKey] = useState("");

  // llama.cpp settings
  const [llamaBinPath, setLlamaBinPath] = useState("llama-server");
  const [llamaPort, setLlamaPort] = useState(8080);
  const [llamaModel, setLlamaModel] = useState("");
  const [llamaThreads, setLlamaThreads] = useState(4);
  const [llamaGpuLayers, setLlamaGpuLayers] = useState(0);
  const [llamaCtxSize, setLlamaCtxSize] = useState(4096);
  const [llamaMmproj, setLlamaMmproj] = useState("");
  const [llamaAutoStart, setLlamaAutoStart] = useState(false);
  const [semanticThreshold, setSemanticThreshold] = useState(0.25);
  const [httpPort, setHttpPort] = useState(8765);

  const [detected, setDetected] = useState<AutoDetect | null>(null);

  const [saved, setSaved] = useState(false);

  const loadSettingsOnce = useCallback(async () => {
    try {
      const settings = await settingsGetAll();
      if (settings.ai_mode) setAiMode(settings.ai_mode as AiMode);
      if (settings.cloud_provider) setCloudProvider(settings.cloud_provider as CloudProvider);
      if (settings.cloud_api_key) setApiKey(settings.cloud_api_key);
      if (settings.llama_bin_path) setLlamaBinPath(settings.llama_bin_path);
      if (settings.llama_port) setLlamaPort(parseInt(settings.llama_port) || 8080);
      if (settings.llama_model) setLlamaModel(settings.llama_model);
      if (settings.llama_threads) setLlamaThreads(parseInt(settings.llama_threads) || 4);
      if (settings.llama_gpu_layers) setLlamaGpuLayers(parseInt(settings.llama_gpu_layers) || 0);
      if (settings.llama_ctx_size) setLlamaCtxSize(parseInt(settings.llama_ctx_size) || 4096);
      if (settings.llama_mmproj) setLlamaMmproj(settings.llama_mmproj);
      if (settings.llama_auto_start) setLlamaAutoStart(settings.llama_auto_start === "true");
      if (settings.semantic_threshold) setSemanticThreshold(parseFloat(settings.semantic_threshold) || 0.25);
      if (settings.http_port) setHttpPort(parseInt(settings.http_port) || 8765);
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }, []);

  const pollStatus = useCallback(async () => {
    try {
      const [status, models, detected] = await Promise.all([
        llamaServerStatus(),
        modelList(),
        autoDetect(),
      ]);
      setServerStatus(status);
      setGgufList(models);
      setDetected(detected);
    } catch (e) {
      console.error("Failed to poll status:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSettingsOnce();
    pollStatus();
    const interval = setInterval(pollStatus, 5000);
    return () => clearInterval(interval);
  }, [loadSettingsOnce, pollStatus]);

  const handleSave = async () => {
    try {
      await settingsSet("ai_mode", aiMode);
      await settingsSet("cloud_provider", cloudProvider);
      await settingsSet("cloud_api_key", apiKey);
      await settingsSet("llama_bin_path", llamaBinPath);
      await settingsSet("llama_port", String(llamaPort));
      await settingsSet("llama_model", llamaModel);
      await settingsSet("llama_threads", String(llamaThreads));
      await settingsSet("llama_gpu_layers", String(llamaGpuLayers));
      await settingsSet("llama_ctx_size", String(llamaCtxSize));
      await settingsSet("llama_mmproj", llamaMmproj);
      await settingsSet("llama_auto_start", llamaAutoStart ? "true" : "false");
      await settingsSet("semantic_threshold", String(semanticThreshold));
      await settingsSet("http_port", String(httpPort));
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  };

  const handleStart = async () => {
    setStarting(true);
    try {
      await handleSave();
      await llamaServerStart();
      await pollStatus();
    } catch (e) {
      console.error("Failed to start server:", e);
      alert(`启动失败: ${e}`);
    } finally {
      setStarting(false);
    }
  };

  const handleStop = async () => {
    setStopping(true);
    try {
      await llamaServerStop();
      await pollStatus();
    } catch (e) {
      console.error("Failed to stop server:", e);
    } finally {
      setStopping(false);
    }
  };

  const showCloudSettings = aiMode === "cloud" || aiMode === "auto";

  return (
    <div className="flex h-full flex-col p-6">
      <h1 className="mb-6 text-2xl font-bold">设置</h1>

      <div className="flex-1 space-y-6 overflow-auto">
        {/* AI Mode */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            AI 推理模式
          </h2>
          <div className="space-y-2">
            {(
              [
                { value: "auto" as AiMode, label: "自动 (Auto)", desc: "优先本地，不可用时降级云端" },
                { value: "local" as AiMode, label: "本地 (Local)", desc: "仅使用本地 llama.cpp" },
                { value: "cloud" as AiMode, label: "云端 (Cloud)", desc: "仅使用云端 API" },
              ] as const
            ).map((opt) => (
              <label
                key={opt.value}
                className="flex cursor-pointer items-start gap-3 rounded border border-transparent p-2 hover:bg-[var(--color-bg-tertiary)]"
              >
                <input
                  type="radio"
                  name="ai_mode"
                  value={opt.value}
                  checked={aiMode === opt.value}
                  onChange={(e) => setAiMode(e.target.value as AiMode)}
                  className="mt-0.5"
                />
                <div>
                  <p className="text-sm text-[var(--color-text-secondary)]">{opt.label}</p>
                  <p className="text-xs text-[var(--color-text-muted)]">{opt.desc}</p>
                </div>
              </label>
            ))}
          </div>
        </section>

        {/* llama.cpp Server */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            llama.cpp 本地服务
          </h2>

          {/* Status */}
          <div className="mb-4 flex items-center gap-3">
            {loading && !serverStatus ? (
              <span className="text-sm text-[var(--color-text-muted)]">检测中...</span>
            ) : serverStatus?.running ? (
              <>
                <span className="h-2 w-2 rounded-full bg-green-500"></span>
                <span className="text-sm text-green-400">
                  运行中 (PID: {serverStatus.pid}, 端口: {serverStatus.port})
                </span>
              </>
            ) : (
              <>
                <span className="h-2 w-2 rounded-full bg-red-500"></span>
                <span className="text-sm text-red-400">已停止</span>
              </>
            )}
          </div>

          {/* Start/Stop buttons */}
          <div className="mb-4 flex gap-2">
            <button
              onClick={handleStart}
              disabled={serverStatus?.running || starting}
              className="rounded bg-green-700 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-green-600 disabled:opacity-50"
            >
              {starting ? "启动中..." : "启动服务"}
            </button>
            <button
              onClick={handleStop}
              disabled={!serverStatus?.running || stopping}
              className="rounded border border-red-800 bg-red-900/30 px-3 py-1.5 text-xs font-medium text-red-400 transition-colors hover:bg-red-900/50 disabled:opacity-50"
            >
              {stopping ? "停止中..." : "停止服务"}
            </button>
          </div>

          {/* Auto-start checkbox */}
          <label className="mb-4 flex cursor-pointer items-center gap-2">
            <input
              type="checkbox"
              checked={llamaAutoStart}
              onChange={(e) => setLlamaAutoStart(e.target.checked)}
              className="h-3.5 w-3.5"
            />
            <span className="text-xs text-[var(--color-text-secondary)]">
              应用启动时自动启动 llama-server
            </span>
          </label>

          {/* Binary path */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              llama-server 二进制路径
            </label>
            {detected && detected.binary_paths.length > 0 ? (
              <select
                value={llamaBinPath}
                onChange={(e) => setLlamaBinPath(e.target.value)}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
              >
                {detected.binary_paths.map((p) => (
                  <option key={p} value={p}>{p}</option>
                ))}
                <option value="">自定路径...</option>
              </select>
            ) : null}
            {(detected?.binary_paths.length ?? 0) === 0 || llamaBinPath === "" || !detected?.binary_paths.includes(llamaBinPath) ? (
              <input
                type="text"
                value={llamaBinPath}
                onChange={(e) => setLlamaBinPath(e.target.value)}
                placeholder="C:\\path\\to\\llama-server.exe"
                className="mt-1 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
            ) : null}
          </div>

          {/* Port */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">端口号</label>
            <input
              type="number"
              value={llamaPort}
              onChange={(e) => setLlamaPort(parseInt(e.target.value) || 8080)}
              className="w-28 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
            />
          </div>

          {/* Threads */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              线程数: {llamaThreads}
            </label>
            <input
              type="range"
              min={1}
              max={16}
              value={llamaThreads}
              onChange={(e) => setLlamaThreads(parseInt(e.target.value))}
              className="w-48"
            />
          </div>

          {/* GPU Layers */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              GPU 层数 (0 = 纯CPU): {llamaGpuLayers}
            </label>
            <input
              type="range"
              min={0}
              max={99}
              step={1}
              value={llamaGpuLayers}
              onChange={(e) => setLlamaGpuLayers(parseInt(e.target.value))}
              className="w-48"
            />
          </div>

          {/* Context Size */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              上下文大小: {llamaCtxSize}
            </label>
            <input
              type="number"
              value={llamaCtxSize}
              onChange={(e) => setLlamaCtxSize(parseInt(e.target.value) || 4096)}
              step={512}
              className="w-28 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
            />
          </div>

        </section>

        {/* Browser Extension */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            浏览器插件
          </h2>
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              HTTP 服务端口
            </label>
            <input
              type="number"
              value={httpPort}
              onChange={(e) => setHttpPort(parseInt(e.target.value) || 8765)}
              className="w-28 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
            />
            <p className="mt-0.5 text-[10px] text-[var(--color-text-muted)]">
              浏览器插件通过此端口与 Medix 通信，修改后需重启应用
            </p>
          </div>
        </section>

        {/* Semantic Search Settings */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            语义搜索
          </h2>
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              最低相似度阈值: {semanticThreshold.toFixed(2)}
            </label>
            <input
              type="range"
              min={0}
              max={0.5}
              step={0.01}
              value={semanticThreshold}
              onChange={(e) => setSemanticThreshold(parseFloat(e.target.value))}
              className="w-48"
            />
            <p className="mt-0.5 text-[10px] text-[var(--color-text-muted)]">
              越高越严格（只返回高度相关的图片），越低越宽松。默认 0.25
            </p>
          </div>
        </section>

        {/* GGUF Models */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            GGUF 模型
          </h2>

          {ggufList && (
            <p className="mb-2 text-xs text-[var(--color-text-muted)]">
              模型目录: {ggufList.models_dir}
            </p>
          )}

          {/* Model selection */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">选择模型</label>
            <select
              value={llamaModel}
              onChange={(e) => setLlamaModel(e.target.value)}
              className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
            >
              <option value="">-- 未选择 --</option>
              {ggufList?.models.map((m) => (
                <option key={m.path} value={m.path}>
                  {m.filename} ({m.size_mb}MB{m.is_vlm ? ", VLM" : ""})
                </option>
              ))}
            </select>
          </div>

          {/* Model files list */}
          {ggufList && ggufList.models.length > 0 && (
            <div className="space-y-1.5">
              <p className="text-xs text-[var(--color-text-muted)]">已发现的模型文件</p>
              {ggufList.models.map((m) => (
                <div
                  key={m.path}
                  className="flex items-center justify-between rounded bg-[var(--color-bg-tertiary)]/50 px-2 py-1.5"
                >
                  <div>
                    <p className="text-xs text-[var(--color-text-secondary)]">{m.filename}</p>
                    <p className="text-[10px] text-[var(--color-text-muted)]">
                      {m.size_mb}MB
                      {m.is_vlm && " · VLM"}
                    </p>
                  </div>
                  {m.path === llamaModel && (
                    <span className="rounded bg-[var(--color-accent-soft)] px-1.5 py-0.5 text-[10px] text-[var(--color-accent)]">
                      已选择
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}

          {ggufList && ggufList.models.length === 0 && (
            <p className="py-2 text-xs text-[var(--color-text-muted)]">
              暂无 GGUF 模型。将 .gguf 文件放入 models 目录即可。
            </p>
          )}

          {/* mmproj (vision projector) */}
          <div className="mb-3 border-t border-[var(--color-border)] pt-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              mmproj (视觉投影器)
            </label>
            {detected && detected.mmproj_files.length > 0 ? (
              <>
                <select
                  value={llamaMmproj}
                  onChange={(e) => setLlamaMmproj(e.target.value)}
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
                >
                  {detected.mmproj_files.map((p) => (
                    <option key={p} value={p}>{p}</option>
                  ))}
                  <option value="">不使用 VLM</option>
                </select>
                {!detected.mmproj_files.includes(llamaMmproj) && llamaMmproj !== "" ? (
                  <input
                    type="text"
                    value={llamaMmproj}
                    onChange={(e) => setLlamaMmproj(e.target.value)}
                    className="mt-1 w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
                  />
                ) : null}
              </>
            ) : (
              <input
                type="text"
                value={llamaMmproj}
                onChange={(e) => setLlamaMmproj(e.target.value)}
                placeholder="留空表示不使用 VLM"
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
            )}
            <p className="mt-0.5 text-[10px] text-[var(--color-text-muted)]">
              将 mmproj 文件放到 models 目录即可自动识别
            </p>
          </div>

          {/* Download hint */}
          <div className="mt-3 rounded bg-[var(--color-bg-tertiary)]/50 p-2">
            <p className="text-[10px] text-[var(--color-text-muted)]">
              下载模型：{" "}
              <a
                href="https://huggingface.co/openbmb/MiniCPM-V-2_6-gguf"
                target="_blank"
                rel="noopener noreferrer"
                className="text-[var(--color-accent)] hover:underline"
              >
                MiniCPM-V 2.6 (VLM, ~1GB Q4)
              </a>
              {" · "}
              <a
                href="https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF"
                target="_blank"
                rel="noopener noreferrer"
                className="text-[var(--color-accent)] hover:underline"
              >
                nomic-embed-text v1.5 (~270MB Q4)
              </a>
            </p>
          </div>
        </section>

        {/* Cloud Settings */}
        {showCloudSettings && (
          <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
            <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
              云端 API 配置
            </h2>
            <div className="space-y-3">
              <div>
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">服务商</label>
                <select
                  value={cloudProvider}
                  onChange={(e) => setCloudProvider(e.target.value as CloudProvider)}
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
                >
                  <option value="claude">Claude (Anthropic)</option>
                  <option value="openai">OpenAI</option>
                  <option value="qwen">Qwen (阿里云)</option>
                </select>
              </div>
              <div>
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">API Key</label>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-..."
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
                />
              </div>
            </div>
          </section>
        )}

        <div className="flex items-center gap-3 pt-2">
          <button
            onClick={handleSave}
            className="rounded bg-[var(--color-accent)] px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)]"
          >
            保存设置
          </button>
          {saved && <span className="text-sm text-green-400">已保存</span>}
        </div>
      </div>
    </div>
  );
}

export default Settings;
