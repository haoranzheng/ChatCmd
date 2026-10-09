import { FolderOpen, LoaderCircle, ShieldAlert } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api } from '../api';
import { tr } from '../i18n';
import type { TaskWorkspaceAccess, WorkspaceAccessMode, WorkspaceProject } from '../types';

/** Only the authenticated local management API can persist this decision. */
export function TaskWorkspaceAccessCard({ taskId, onBound }: {
  taskId: string;
  onBound: (folder: string) => void;
}) {
  const [projects, setProjects] = useState<WorkspaceProject[]>([]);
  const [current, setCurrent] = useState<TaskWorkspaceAccess | null>(null);
  const [projectId, setProjectId] = useState('');
  const [accessMode, setAccessMode] = useState<WorkspaceAccessMode>('readOnly');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    setError('');
    setNotice('');
    void Promise.all([api.workspaceProjects(), api.taskWorkspace(taskId)])
      .then(([saved, binding]) => {
        if (!mounted) return;
        setProjects(saved);
        setCurrent(binding);
        setProjectId(binding.projectId ?? '');
        setAccessMode(binding.locallyAuthorized ? binding.accessMode : 'readOnly');
      })
      .catch((reason: unknown) => {
        if (mounted) setError(reason instanceof Error ? reason.message : tr('Could not load workspace permissions.'));
      })
      .finally(() => { if (mounted) setLoading(false); });
    return () => { mounted = false; };
  }, [taskId]);

  const selected = projects.find((project) => project.id === projectId);
  const changed = projectId !== (current?.projectId ?? '') || accessMode !== (current?.accessMode ?? 'readOnly') || !current?.locallyAuthorized;
  async function save() {
    if (!selected || saving || selected.pathValid === false) return;
    const scopeChanged = current?.projectId !== selected.id;
    if ((scopeChanged || accessMode === 'readWrite') && !window.confirm(
      tr('Confirm access for the selected local folder: {path}. This does not grant access to parent or sibling folders.', { path: selected.path })
    )) return;
    setSaving(true);
    setError('');
    setNotice('');
    try {
      const binding = await api.setTaskWorkspace(taskId, { projectId, accessMode });
      setCurrent(binding);
      onBound(binding.projectFolder ?? '');
      setNotice(tr('Workspace binding saved. Pending approvals and old scoped grants were invalidated.'));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : tr('Could not update workspace permissions.'));
    } finally {
      setSaving(false);
    }
  }

  return <section className="task-access-card" aria-label={tr('Workspace scope and file access')}>
    <header><FolderOpen aria-hidden="true" /><div><h3>{tr('Workspace scope and file access')}</h3><p>{tr('Workspace folder, tool allowlist and individual execution approval are separate controls.')}</p></div></header>
    {loading ? <p role="status"><LoaderCircle className="spin" />{tr('Loading projects…')}</p> : <>
      <p>{current?.locallyAuthorized
        ? tr('Current access: {mode}', { mode: tr(current.accessMode) })
        : tr('This task has no locally approved workspace binding. Select a project below.')}</p>
      {current?.projectFolder && <p><small>{current.projectFolder}</small></p>}
      <label htmlFor={`workspace-project-${taskId}`}>{tr('Workspace project')}</label>
      <select id={`workspace-project-${taskId}`} value={projectId} onChange={(event) => {
        setProjectId(event.target.value); setAccessMode('readOnly'); setNotice('');
      }} disabled={saving}>
        <option value="">{tr('Select a saved project')}</option>
        {projects.map((project) => <option value={project.id} key={project.id}>{project.name} — {project.path}</option>)}
      </select>
      {selected?.pathValid === false && <p role="alert" className="task-access-error">{tr('Project path is missing or unsafe. Edit the project folder before binding.')}</p>}
      {selected && <><label htmlFor={`workspace-mode-${taskId}`}>{tr('File access mode')}</label>
        <select id={`workspace-mode-${taskId}`} value={accessMode} onChange={(event) => setAccessMode(event.target.value as WorkspaceAccessMode)} disabled={saving}>
          <option value="restricted">{tr('restricted')}</option>
          <option value="readOnly">{tr('readOnly')}</option>
          <option value="readWrite">{tr('readWrite')}</option>
        </select>
      </>}
      {accessMode === 'readWrite' && <p><ShieldAlert aria-hidden="true" />{tr('File changes still require per-operation approval. Shell commands are not an OS sandbox.')}</p>}
      {!projects.length && <p>{tr('Create a workspace project with the folder picker in the left Projects rail.')}</p>}
      {error && <p role="alert" className="task-access-error">{error} {tr('Open the local ChatCMD page and sign in to approve this change.')}</p>}
      {notice && <p role="status">{notice}</p>}
      <button type="button" className="button primary compact" disabled={!selected || !changed || saving || selected.pathValid === false} onClick={() => void save()}>
        {saving ? tr('Saving…') : tr('Save workspace access')}
      </button>
    </>}
  </section>;
}
