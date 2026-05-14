import { useCallback, useEffect, useState } from "react";
import type { LlamaServerStatus, GgufModelList } from "@/types/ai";
import {
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

  const [saved, setSaved] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [status, models, settings] = await Promise.all([
        llamaServerStatus(),
        modelList(),
        settingsGetAll(),
      ]);
      setServerStatus(status);
      setGgufList(models);

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
    } catch (e) {
      console.error("Failed to load settings:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 5000);
    return () => clearInterval(interval);
  }, [loadData]);

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
      await loadData();
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
      await loadData();
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
        <section className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <h2 className="mb-3 text-sm font-semibold text-neutral-200">
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
                className="flex cursor-pointer items-start gap-3 rounded border border-transparent p-2 hover:bg-neutral-800"
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
                  <p className="text-sm text-neutral-300">{opt.label}</p>
                  <p className="text-xs text-neutral-500">{opt.desc}</p>
                </div>
              </label>
            ))}
          </div>
        </section>

        {/* llama.cpp Server */}
        <section className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <h2 className="mb-3 text-sm font-semibold text-neutral-200">
            llama.cpp 本地服务
          </h2>

          {/* Status */}
          <div className="mb-4 flex items-center gap-3">
            {loading && !serverStatus ? (
              <span className="text-sm text-neutral-500">检测中...</span>
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

          {/* Binary path */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-neutral-500">
              llama-server 二进制路径
            </label>
            <input
              type="text"
              value={llamaBinPath}
              onChange={(e) => setLlamaBinPath(e.target.value)}
              placeholder="llama-server"
              className="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none placeholder:text-neutral-600"
            />
          </div>

          {/* Port */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-neutral-500">端口号</label>
            <input
              type="number"
              value={llamaPort}
              onChange={(e) => setLlamaPort(parseInt(e.target.value) || 8080)}
              className="w-28 rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none"
            />
          </div>

          {/* Threads */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-neutral-500">
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
            <label className="mb-1 block text-xs text-neutral-500">
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
            <label className="mb-1 block text-xs text-neutral-500">
              上下文大小: {llamaCtxSize}
            </label>
            <input
              type="number"
              value={llamaCtxSize}
              onChange={(e) => setLlamaCtxSize(parseInt(e.target.value) || 4096)}
              step={512}
              className="w-28 rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none"
            />
          </div>

          {/* mmproj (vision projector) */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-neutral-500">
              mmproj (视觉投影器)
            </label>
            <input
              type="text"
              value={llamaMmproj}
              onChange={(e) => setLlamaMmproj(e.target.value)}
              placeholder="留空表示不使用 VLM"
              className="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none placeholder:text-neutral-600"
            />
            <p className="mt-0.5 text-[10px] text-neutral-500">
              VLM 需要单独的 mmproj 文件，如 mmproj-MiniCPM-V-2_6-f16.gguf
            </p>
          </div>
        </section>

        {/* GGUF Models */}
        <section className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <h2 className="mb-3 text-sm font-semibold text-neutral-200">
            GGUF 模型
          </h2>

          {ggufList && (
            <p className="mb-2 text-xs text-neutral-500">
              模型目录: {ggufList.models_dir}
            </p>
          )}

          {/* Model selection */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-neutral-500">选择模型</label>
            <select
              value={llamaModel}
              onChange={(e) => setLlamaModel(e.target.value)}
              className="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none"
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
              <p className="text-xs text-neutral-500">已发现的模型文件</p>
              {ggufList.models.map((m) => (
                <div
                  key={m.path}
                  className="flex items-center justify-between rounded bg-neutral-800/50 px-2 py-1.5"
                >
                  <div>
                    <p className="text-xs text-neutral-300">{m.filename}</p>
                    <p className="text-[10px] text-neutral-500">
                      {m.size_mb}MB
                      {m.is_vlm && " · VLM"}
                    </p>
                  </div>
                  {m.path === llamaModel && (
                    <span className="rounded bg-blue-900/30 px-1.5 py-0.5 text-[10px] text-blue-400">
                      已选择
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}

          {ggufList && ggufList.models.length === 0 && (
            <p className="py-2 text-xs text-neutral-600">
              暂无 GGUF 模型。将 .gguf 文件放入 models 目录即可。
            </p>
          )}

          {/* Download hint */}
          <div className="mt-3 rounded bg-neutral-800/50 p-2">
            <p className="text-[10px] text-neutral-500">
              下载模型：{" "}
              <a
                href="https://huggingface.co/openbmb/MiniCPM-V-2_6-gguf"
                target="_blank"
                rel="noopener noreferrer"
                className="text-blue-400 hover:underline"
              >
                MiniCPM-V 2.6 (VLM, ~1GB Q4)
              </a>
              {" · "}
              <a
                href="https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF"
                target="_blank"
                rel="noopener noreferrer"
                className="text-blue-400 hover:underline"
              >
                nomic-embed-text v1.5 (~270MB Q4)
              </a>
            </p>
          </div>
        </section>

        {/* Cloud Settings */}
        {showCloudSettings && (
          <section className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
            <h2 className="mb-3 text-sm font-semibold text-neutral-200">
              云端 API 配置
            </h2>
            <div className="space-y-3">
              <div>
                <label className="mb-1 block text-xs text-neutral-500">服务商</label>
                <select
                  value={cloudProvider}
                  onChange={(e) => setCloudProvider(e.target.value as CloudProvider)}
                  className="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none"
                >
                  <option value="claude">Claude (Anthropic)</option>
                  <option value="openai">OpenAI</option>
                  <option value="qwen">Qwen (阿里云)</option>
                </select>
              </div>
              <div>
                <label className="mb-1 block text-xs text-neutral-500">API Key</label>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-..."
                  className="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none placeholder:text-neutral-600"
                />
              </div>
            </div>
          </section>
        )}

        <div className="flex items-center gap-3 pt-2">
          <button
            onClick={handleSave}
            className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
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
