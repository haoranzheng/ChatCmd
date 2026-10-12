import { tr } from '../i18n';

export interface HighRiskApproval {
  title: string;
  consequence: string;
  paths: string[];
  pathCount: number;
  recursive: boolean;
  overwrite: boolean;
  mode: string | null;
  operation: string;
  source: string | null;
  destination: string | null;
  dryRun: boolean;
  contentBytesEstimate: number | null;
  expectedVersion: boolean;
  commandPreview: string | null;
  commandReason: string | null;
  previewTruncated: boolean;
  previewRedacted: boolean;
}

/**
 * Explicit list shared in intent with the server's independent allow gate.
 * Do not infer a safe decision from UI data or risk text.
 */
const HIGH_RISK_TOOLS: Record<string, [string, string]> = {
  fs_delete: ['Delete files or folders', 'Files could be removed permanently. Confirm the exact target and deletion mode.'],
  fs_move: ['Move or rename files', 'The original location will change. Existing files at the destination may be affected.'],
  fs_copy: ['Copy files or folders', 'The destination may be overwritten. Confirm both source and destination.'],
  fs_write_raw: ['Write binary data', 'This can replace file bytes without a readable text diff. Incorrect data may be difficult to recover.'],
  fs_quarantine_gc: ['Permanently clear quarantined files', 'Previously quarantined content may become unrecoverable.'],
  fs_restore_quarantine: ['Restore quarantined files', 'Restoring files can replace or conflict with current files.'],
  process_kill: ['Force-stop a process', 'Unsaved work in that process may be lost.'],
  shell_force_close: ['Force-close a terminal', 'Running commands may stop abruptly and leave partial changes.'],
  git_commit: ['Create a Git commit', 'This records changes in repository history. Inspect the staged changes before proceeding.'],
  git_push: ['Push Git changes', 'This can publish local commits to a remote repository. Verify branch and remote.'],
};

export function highRiskApproval(tool: string | undefined, rawSummary: unknown): HighRiskApproval | null {
  const summary = rawSummary && typeof rawSummary === 'object' && !Array.isArray(rawSummary)
    ? rawSummary as Record<string, unknown> : {};
  const operation = tool || (typeof summary.operation === 'string' ? summary.operation : '');
  const command = summary.command && typeof summary.command === 'object'
    ? summary.command as Record<string, unknown> : {};
  const serverMarkedShell = ['command_run','shell_create','shell_write'].includes(operation) && summary.highRisk === true;
  const details = HIGH_RISK_TOOLS[operation] ?? (serverMarkedShell
    ? ['Review risky command','Shell command or script may delete data or execute code. Verify the command before approving.'] as [string,string] : undefined);
  if (!details) return null;
  const paths = Array.isArray(summary.paths) ? summary.paths.filter((p): p is string => typeof p === 'string' && p.length > 0) : [];
  if (!paths.length) {
    for (const key of ['source', 'destination', 'path']) {
      if (typeof summary[key] === 'string' && summary[key]) paths.push(summary[key]);
    }
  }
  const count = typeof summary.pathCount === 'number' && Number.isFinite(summary.pathCount)
    ? Math.max(paths.length, Math.max(0, Math.trunc(summary.pathCount))) : paths.length;
  return {
    title: tr(details[0]),
    consequence: tr(details[1]),
    paths: paths.slice(0, 8),
    pathCount: count,
    recursive: summary.recursive === true,
    overwrite: summary.overwrite === true,
    mode: typeof summary.deleteMode === 'string' ? summary.deleteMode : null,
    operation,
    source: typeof summary.source === 'string' && summary.source ? summary.source : (['fs_move','fs_copy'].includes(operation) ? paths[0] ?? null : null),
    destination: typeof summary.destination === 'string' && summary.destination ? summary.destination : (['fs_move','fs_copy'].includes(operation) ? paths[1] ?? null : null),
    dryRun: summary.dryRun === true,
    contentBytesEstimate: typeof summary.contentBytesEstimate === 'number' && Number.isFinite(summary.contentBytesEstimate)
      ? Math.max(0, Math.trunc(summary.contentBytesEstimate)) : null,
    expectedVersion: typeof summary.expectedVersion === 'string' && summary.expectedVersion.length > 0,
    commandPreview: typeof command.preview === 'string' ? command.preview : null,
    commandReason: typeof summary.highRiskReason === 'string' ? tr(summary.highRiskReason) : null,
    previewTruncated: command.previewTruncated === true,
    previewRedacted: command.previewRedacted === true,
  };
}
