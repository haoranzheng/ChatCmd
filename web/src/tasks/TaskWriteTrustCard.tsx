import { useEffect, useState } from 'react';
import { FilePenLine, LoaderCircle, ShieldAlert, ShieldX } from 'lucide-react';
import { api, type WriteTrust } from '../api';
import { tr } from '../i18n';

export function TaskWriteTrustCard({ taskId }: { taskId: string }) {
  const [trust, setTrust] = useState<WriteTrust | null>(null);
  const [durationMinutes, setDuration] = useState<15 | 60>(60);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let mounted = true;
    setTrust(null);
    void api.taskWriteTrust(taskId).then(v => { if(mounted) setTrust(v); })
      .catch(() => { if(mounted) setError(tr('Unable to load workspace write trust.')); });
    return () => { mounted = false; };
  }, [taskId]);
  useEffect(() => {
    if (!trust?.active || trust.expiresAtMs === null) return;
    const timer = window.setTimeout(() => {
      setTrust(v => v?.expiresAtMs === trust.expiresAtMs
        ? { ...v, active:false, expiresAtMs:null } : v);
    }, Math.max(0, trust.expiresAtMs - Date.now()));
    return () => window.clearTimeout(timer);
  }, [trust]);
  async function grant() {
    if (busy || !window.confirm(tr('Trust this conversation to edit approved workspace text files without individual approvals for the selected period? This can modify code and data.'))) return;
    setBusy(true); setError('');
    try { setTrust(await api.grantTaskWriteTrust(taskId,durationMinutes)); }
    catch (e) { setError(e instanceof Error ? e.message : tr('Unable to grant workspace write trust.')); }
    finally { setBusy(false); }
  }
  async function revoke() {
    if (busy) return;
    setBusy(true); setError('');
    try { setTrust(await api.revokeTaskWriteTrust(taskId)); }
    catch (e) { setError(e instanceof Error ? e.message : tr('Unable to revoke workspace write trust.')); }
    finally { setBusy(false); }
  }
  return <section className="task-access-card" aria-label={tr('Trusted workspace edits')}>
    <header><FilePenLine aria-hidden="true" /><div>
      <h3>{tr('Trusted workspace edits')}</h3>
      <p>{tr('Optional: skip repeated approvals for bounded text edits in this conversation only.')}</p>
    </div></header>
    {trust?.active ? <>
      <p role="status">{tr('Trusted edits: {used} / {max} calls', {used:trust.usedCalls,max:trust.maxCalls})}
        {' · '}{new Date(trust.expiresAtMs ?? 0).toLocaleString()}</p>
      <button type="button" className="button danger compact" disabled={busy} onClick={() => void revoke()}>
        <ShieldX /> {tr('Revoke write trust')}
      </button>
    </> : <>
      <label htmlFor={`write-trust-duration-${taskId}`}>{tr('Trust duration')}</label>
      <select id={`write-trust-duration-${taskId}`} value={durationMinutes}
        disabled={busy || trust===null} onChange={e => setDuration(Number(e.target.value) as 15|60)}>
        <option value={15}>{tr('15 minutes')}</option><option value={60}>{tr('1 hour')}</option>
      </select>
      <p><ShieldAlert aria-hidden="true" />{tr('Read-write workspace binding is required. Up to 128 text edits; no delete, move, binary writes, shell, Git commit, or protected files. Revoke anytime.')}</p>
      <button type="button" className="button primary compact" disabled={busy || trust===null} onClick={() => void grant()}>
        {busy?<LoaderCircle className="spin" />:<FilePenLine />} {tr('Trust text edits')}
      </button>
    </>}
    {error && <p role="alert" className="task-access-error">{error}</p>}
  </section>;
}
