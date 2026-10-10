import { useEffect, useState } from 'react';
import { LoaderCircle, MonitorCheck, ShieldAlert, ShieldX } from 'lucide-react';
import { api, type DesktopTrust } from '../api';
import { tr } from '../i18n';

export function TaskDesktopTrustCard({ taskId }: { taskId: string }) {
  const [trust, setTrust] = useState<DesktopTrust | null>(null);
  const [scope, setScope] = useState<'observe' | 'control'>('observe');
  const [durationMinutes, setDuration] = useState<15 | 60 | 480>(60);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    let active = true;
    setTrust(null);
    setError('');
    void api.taskDesktopTrust(taskId).then((value) => { if (active) setTrust(value); })
      .catch(() => { if (active) setError(tr('Unable to load desktop trust.')); });
    return () => { active = false; };
  }, [taskId]);

  async function grant() {
    if (busy) return;
    const text = scope === 'control'
      ? tr('Trust this conversation to view your screen and control Windows without individual prompts for the chosen period?')
      : tr('Trust this conversation to view screen contents without individual prompts for the chosen period?');
    if (!window.confirm(text)) return;
    setBusy(true);
    setError('');
    try { setTrust(await api.grantTaskDesktopTrust(taskId, { scope, durationMinutes })); }
    catch (reason) { setError(reason instanceof Error ? reason.message : tr('Unable to grant desktop trust.')); }
    finally { setBusy(false); }
  }
  async function revoke() {
    if (busy) return;
    setBusy(true); setError('');
    try { setTrust(await api.revokeTaskDesktopTrust(taskId)); }
    catch (reason) { setError(reason instanceof Error ? reason.message : tr('Unable to revoke desktop trust.')); }
    finally { setBusy(false); }
  }

  return <section className="task-access-card" aria-label={tr('Windows desktop trust')}>
    <header><MonitorCheck aria-hidden="true" /><div><h3>{tr('Windows desktop trust')}</h3>
      <p>{tr('Only this conversation. Does not grant filesystem or command permissions.')}</p></div></header>
    {trust?.active ? <>
      <p role="status">{tr('Trusted: {scope}', { scope: tr(trust.scope) })} · {tr('Expires: {time}', { time: new Date(trust.expiresAtMs ?? 0).toLocaleString() })}</p>
      <button type="button" className="button danger compact" disabled={busy} onClick={() => void revoke()}><ShieldX /> {tr('Revoke desktop trust')}</button>
    </> : <>
      <label htmlFor={`desktop-scope-${taskId}`}>{tr('Trusted actions')}</label>
      <select id={`desktop-scope-${taskId}`} value={scope} onChange={(event) => setScope(event.target.value as 'observe'|'control')} disabled={busy || trust === null}>
        <option value="observe">{tr('View screen only')}</option>
        <option value="control">{tr('View and control Windows')}</option>
      </select>
      <label htmlFor={`desktop-duration-${taskId}`}>{tr('Trust duration')}</label>
      <select id={`desktop-duration-${taskId}`} value={durationMinutes} disabled={busy || trust === null} onChange={(event) => setDuration(Number(event.target.value) as 15|60|480)}>
        <option value={15}>{tr('15 minutes')}</option>
        <option value={60}>{tr('1 hour')}</option>
        <option value={480}>{tr('8 hours')}</option>
      </select>
      <p><ShieldAlert aria-hidden="true" />{tr('Only enable for conversations you trust. Screenshots can expose secrets, and desktop control can change files through applications.')}</p>
      <button type="button" className="button primary compact" disabled={busy || trust === null} onClick={() => void grant()}>
        {busy ? <LoaderCircle className="spin" /> : <MonitorCheck />} {tr('Trust this conversation')}
      </button>
    </>}
    {error && <p role="alert" className="task-access-error">{error} {tr('Check Windows-MCP status under Settings > Security.')}</p>}
  </section>;
}
