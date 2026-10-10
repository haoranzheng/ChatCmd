import { FolderOpen, LoaderCircle, Plus, ShieldAlert } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api } from '../api';
import { tr } from '../i18n';
import type { TaskWorkspaceAccess, WorkspaceAccessMode, WorkspaceProject } from '../types';

const MAX_ADDITIONAL_FOLDERS = 15;
const sameIds = (a: string[], b: string[]) => a.length === b.length && a.every((id) => b.includes(id));
const samePath = (a: string, b: string) => a.replace(/\\/g, '/').replace(/\/$/, '').toLowerCase() === b.replace(/\\/g, '/').replace(/\/$/, '').toLowerCase();

/** Only the authenticated local management API can persist this decision. */
export function TaskWorkspaceAccessCard({ taskId, onBound }: {
  taskId: string;
  onBound: (folder: string) => void;
}) {
  const [projects, setProjects] = useState<WorkspaceProject[]>([]);
  const [current, setCurrent] = useState<TaskWorkspaceAccess | null>(null);
  const [projectId, setProjectId] = useState('');
  const [additionalProjectIds, setAdditionalProjectIds] = useState<string[]>([]);
  const [accessMode, setAccessMode] = useState<WorkspaceAccessMode>('readOnly');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [adding, setAdding] = useState(false);
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
        setAdditionalProjectIds(binding.additionalProjectIds ?? []);
        setAccessMode(binding.locallyAuthorized ? binding.accessMode : 'readOnly');
      })
      .catch((reason: unknown) => {
        if (mounted) setError(reason instanceof Error ? reason.message : tr('Could not load workspace permissions.'));
      })
      .finally(() => { if (mounted) setLoading(false); });
    return () => { mounted = false; };
  }, [taskId]);

  const selected = projects.find((project) => project.id === projectId);
  const additional = projects.filter((project) => additionalProjectIds.includes(project.id));
  const invalidAdditional = additional.length !== additionalProjectIds.length || additional.some((project) => project.pathValid === false);
  const changed = projectId !== (current?.projectId ?? '')
    || accessMode !== (current?.accessMode ?? 'readOnly')
    || !sameIds(additionalProjectIds, current?.additionalProjectIds ?? [])
    || !current?.locallyAuthorized;

  async function addFolder() {
    if (adding || saving) return;
    setAdding(true);
    setError('');
    try {
      const picked = await api.pickProjectFolder();
      if (!picked.path) return;
      const folder = picked.path;
      let saved = projects.find((project) => samePath(project.path, folder));
      if (!saved) {
        const name = folder.split(/[\\/]/).filter(Boolean).at(-1) ?? 'Workspace';
        saved = await api.saveWorkspaceProject({ name, path: folder });
      }
      setProjects(await api.workspaceProjects());
      if (!projectId) {
        setProjectId(saved.id);
      } else if (saved.id !== projectId) {
        setAdditionalProjectIds((ids) => ids.includes(saved.id) || ids.length >= MAX_ADDITIONAL_FOLDERS ? ids : [...ids, saved.id]);
      }
      setNotice(tr('Folder added. Save workspace access to authorize it.'));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : tr('Could not update workspace permissions.'));
    } finally {
      setAdding(false);
    }
  }

  async function save() {
    if (!selected || saving || adding || selected.pathValid === false || invalidAdditional) return;
    const scopeChanged = current?.projectId !== selected.id || !sameIds(additionalProjectIds, current?.additionalProjectIds ?? []);
    const paths = [selected, ...additional].map((project) => project.path).join('\n');
    if ((scopeChanged || accessMode === 'readWrite') && !window.confirm(
      tr('Confirm access to these local folders: {paths}. Parent and sibling folders remain excluded.', { paths })
    )) return;
    setSaving(true);
    setError('');
    setNotice('');
    try {
      const binding = await api.setTaskWorkspace(taskId, { projectId, additionalProjectIds, accessMode });
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
        setProjectId(event.target.value);
        setAdditionalProjectIds((ids) => ids.filter((id) => id !== event.target.value));
        setAccessMode('readOnly');
        setNotice('');
      }} disabled={saving || adding}>
        <option value="">{tr('Select a saved project')}</option>
        {projects.map((project) => <option value={project.id} key={project.id}>{project.name} — {project.path}</option>)}
      </select>
      {selected?.pathValid === false && <p role="alert" className="task-access-error">{tr('Project path is missing or unsafe. Edit the project folder before binding.')}</p>}
      <fieldset className="task-extra-roots" disabled={!selected || saving || adding}>
        <legend>{tr('Additional folders (up to 15)')}</legend>
        <p><small>{tr('Only explicitly selected folders are accessible. Relative paths use the primary project above.')}</small></p>
        {projects.filter((project) => project.id !== projectId).map((project) =>
          <label className="task-extra-root" key={project.id}>
            <input type="checkbox" checked={additionalProjectIds.includes(project.id)}
              disabled={project.pathValid === false || (!additionalProjectIds.includes(project.id) && additionalProjectIds.length >= MAX_ADDITIONAL_FOLDERS)}
              onChange={(event) => {
                setAdditionalProjectIds((ids) => event.target.checked ? [...ids, project.id] : ids.filter((id) => id !== project.id));
                setNotice('');
              }} />
            <span><strong>{project.name}</strong><small>{project.path}</small></span>
          </label>
        )}
      </fieldset>
      <button type="button" className="button secondary compact task-add-root" onClick={() => void addFolder()} disabled={saving || adding || (!!projectId && additionalProjectIds.length >= MAX_ADDITIONAL_FOLDERS)}>
        {adding ? <LoaderCircle className="spin" aria-hidden="true" /> : <Plus aria-hidden="true" />} {tr('Add local folder')}
      </button>
      {selected && <><label htmlFor={`workspace-mode-${taskId}`}>{tr('File access mode')}</label>
        <select id={`workspace-mode-${taskId}`} value={accessMode} onChange={(event) => setAccessMode(event.target.value as WorkspaceAccessMode)} disabled={saving || adding}>
          <option value="restricted">{tr('restricted')}</option>
          <option value="readOnly">{tr('readOnly')}</option>
          <option value="readWrite">{tr('readWrite')}</option>
        </select>
        <p><small>{tr('This access mode applies to every selected folder.')}</small></p>
      </>}
      {accessMode === 'readWrite' && <p><ShieldAlert aria-hidden="true" />{tr('File changes still require per-operation approval. Shell commands are not an OS sandbox.')}</p>}
      {!projects.length && <p>{tr('Create a workspace project with the folder picker in the left Projects rail.')}</p>}
      {invalidAdditional && <p role="alert" className="task-access-error">{tr('Project path is missing or unsafe. Edit the project folder before binding.')}</p>}
      {error && <p role="alert" className="task-access-error">{error} {tr('Open the local ChatCMD page and sign in to approve this change.')}</p>}
      {notice && <p role="status">{notice}</p>}
      <button type="button" className="button primary compact" disabled={!selected || !changed || saving || adding || invalidAdditional || selected.pathValid === false} onClick={() => void save()}>
        {saving ? tr('Saving…') : tr('Save workspace access')}
      </button>
    </>}
  </section>;
}
