<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/tauri/commands";
  import type { InsightsSummary } from "$lib/tauri/commands";
  import { formatNumber } from "$lib/utils/format";
  import { MessageSquare, Layers, Wrench, Calendar, FolderTree, BookText, Clock, ListChecks } from "lucide-svelte";

  let data = $state<InsightsSummary | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      data = await api.insights.compute();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  const maxToolCount = $derived(data?.toolFrequencies[0]?.count ?? 1);
  const maxProjectCount = $derived(data?.topProjects[0]?.messageCount ?? 1);
</script>

<div class="p-6 space-y-6 overflow-y-auto h-full">
  <header class="flex items-end justify-between">
    <div>
      <h1 class="text-2xl font-semibold text-text-primary">Insights</h1>
      <p class="text-sm text-text-muted mt-1">
        Aggregate signal from your Claude Code history. All computed locally — no network, no LLM.
      </p>
    </div>
    <button
      class="px-3 py-1.5 text-xs bg-bg-secondary border border-border rounded-md hover:bg-bg-tertiary transition-colors"
      onclick={load}
      disabled={loading}
    >
      {loading ? "Computing…" : "Refresh"}
    </button>
  </header>

  {#if loading && !data}
    <div class="flex items-center justify-center h-64">
      <p class="text-text-muted">Walking transcripts…</p>
    </div>
  {:else if error}
    <div class="bg-danger/10 border border-danger/30 text-danger rounded-lg p-4 text-sm">
      Failed to compute insights: {error}
    </div>
  {:else if data}
    <!-- Headline numbers -->
    <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
      <div class="bg-bg-secondary border border-border rounded-lg p-4">
        <div class="flex items-center gap-2 text-xs text-text-muted">
          <MessageSquare size={14} /> Messages
        </div>
        <div class="text-2xl font-semibold text-text-primary mt-1">{formatNumber(data.totalMessages)}</div>
        <div class="text-xs text-text-muted mt-0.5">user + assistant turns</div>
      </div>
      <div class="bg-bg-secondary border border-border rounded-lg p-4">
        <div class="flex items-center gap-2 text-xs text-text-muted">
          <Layers size={14} /> Sessions
        </div>
        <div class="text-2xl font-semibold text-text-primary mt-1">{formatNumber(data.totalSessions)}</div>
        <div class="text-xs text-text-muted mt-0.5">distinct conversations</div>
      </div>
      <div class="bg-bg-secondary border border-border rounded-lg p-4">
        <div class="flex items-center gap-2 text-xs text-text-muted">
          <Wrench size={14} /> Tool calls
        </div>
        <div class="text-2xl font-semibold text-text-primary mt-1">{formatNumber(data.totalToolCalls)}</div>
        <div class="text-xs text-text-muted mt-0.5">across all sessions</div>
      </div>
      <div class="bg-bg-secondary border border-border rounded-lg p-4">
        <div class="flex items-center gap-2 text-xs text-text-muted">
          <Calendar size={14} /> Active days
        </div>
        <div class="text-2xl font-semibold text-text-primary mt-1">{data.activeDays}</div>
        <div class="text-xs text-text-muted mt-0.5">{data.firstDate} → {data.lastDate}</div>
      </div>
    </div>

    <!-- Resource counts (one-off scalars from local fs) -->
    <div class="grid grid-cols-3 gap-3">
      <div class="bg-bg-secondary border border-border rounded-lg p-3 flex items-center gap-3">
        <ListChecks size={20} class="text-accent shrink-0" />
        <div>
          <div class="text-lg font-semibold text-text-primary">{data.plansCount}</div>
          <div class="text-xs text-text-muted">plan files</div>
        </div>
      </div>
      <div class="bg-bg-secondary border border-border rounded-lg p-3 flex items-center gap-3">
        <Clock size={20} class="text-accent shrink-0" />
        <div>
          <div class="text-lg font-semibold text-text-primary">{data.scheduledTasksCount}</div>
          <div class="text-xs text-text-muted">scheduled tasks</div>
        </div>
      </div>
      <div class="bg-bg-secondary border border-border rounded-lg p-3 flex items-center gap-3">
        <BookText size={20} class="text-accent shrink-0" />
        <div>
          <div class="text-lg font-semibold text-text-primary">{data.memoryFilesCount}</div>
          <div class="text-xs text-text-muted">memory files (across projects)</div>
        </div>
      </div>
    </div>

    <!-- Top projects -->
    <section class="bg-bg-secondary border border-border rounded-lg p-4">
      <h2 class="text-sm font-medium text-text-secondary mb-3 flex items-center gap-1.5">
        <FolderTree size={14} />
        Top projects by activity
      </h2>
      <div class="space-y-2">
        {#each data.topProjects as p}
          <div class="flex items-center gap-3 text-sm">
            <div class="flex-1 min-w-0">
              <div class="text-text-primary truncate" title={p.slug}>{p.displayName}</div>
              <div class="h-1.5 bg-bg-tertiary rounded-full overflow-hidden mt-1">
                <div
                  class="h-full bg-accent rounded-full"
                  style="width: {Math.max(2, (p.messageCount / maxProjectCount) * 100)}%"
                ></div>
              </div>
            </div>
            <div class="text-right text-xs shrink-0 w-28">
              <div class="text-text-primary font-medium">{formatNumber(p.messageCount)} msgs</div>
              <div class="text-text-muted">{p.sessionCount} sessions · {p.lastActivity}</div>
            </div>
          </div>
        {/each}
      </div>
    </section>

    <!-- Clusters -->
    {#if data.projectClusters.length > 0}
      <section class="bg-bg-secondary border border-border rounded-lg p-4">
        <h2 class="text-sm font-medium text-text-secondary mb-3">Project clusters</h2>
        <p class="text-xs text-text-muted mb-3">
          Grouped by the token after <code class="bg-bg-tertiary px-1 rounded">ACTION-</code> in the project slug. Helps spot domains where you do similar work.
        </p>
        <div class="grid grid-cols-2 md:grid-cols-3 gap-2">
          {#each data.projectClusters as c}
            <div class="bg-bg-tertiary border border-border rounded p-3">
              <div class="text-sm font-medium text-text-primary">{c.key}</div>
              <div class="text-xs text-text-muted mt-1">
                {c.projectCount} project{c.projectCount === 1 ? "" : "s"} · {formatNumber(c.messageCount)} msgs
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <!-- Tool frequencies -->
    {#if data.toolFrequencies.length > 0}
      <section class="bg-bg-secondary border border-border rounded-lg p-4">
        <h2 class="text-sm font-medium text-text-secondary mb-3 flex items-center gap-1.5">
          <Wrench size={14} />
          Most-used tools
        </h2>
        <div class="space-y-1.5">
          {#each data.toolFrequencies as t}
            <div class="flex items-center gap-3 text-sm">
              <div class="w-32 shrink-0 text-text-primary truncate" title={t.name}>{t.name}</div>
              <div class="flex-1 h-1.5 bg-bg-tertiary rounded-full overflow-hidden">
                <div
                  class="h-full bg-accent rounded-full"
                  style="width: {Math.max(2, (t.count / maxToolCount) * 100)}%"
                ></div>
              </div>
              <div class="text-xs text-text-muted shrink-0 w-16 text-right">{formatNumber(t.count)}</div>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <p class="text-xs text-text-muted text-center pt-2">
      Generated {data.generatedAt} · 100% local · re-run anytime with Refresh
    </p>
  {/if}
</div>
