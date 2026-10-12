import { useEffect, useState } from 'react';
import { FilePenLine, LoaderCircle, RefreshCw, ShieldAlert, ShieldX } from 'lucide-react';
import { api, type WriteTrust } from '../api';
import { tr } from '../i18n';

/** Poll only when visible. Calls are read-only and grant/revoke are never initiated by polling. */
export function TaskWriteTrustCard({ taskId }: { taskId: string }) {
  const [trust, setTrust] = useState<WriteTrust | null>(null);
  const [durationMinutes, setDuration] = useState<15 | 60>(60);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [now, setNow] = useState(0);
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let active = true;
    let fetching = false;
    setTrust(null); setError('');
    const sync = async () => {
      if (fetching || document.visibilityState === 'hidden') return;
      fetching = true;
      try { const v = await api.taskWriteTrust(taskId); if (active) { setTrust(v); setNow(Date.now()); setError(''); } }
      catch { if (active) setError(tr('Unable to load workspace write trust.')); }
      finally { fetching = false; }
    };
    void sync();
    const id = window.setInterval(() => { if (active) void sync(); }, 5000);
    const visible = () => { if (document.visibilityState === 'visible') void sync(); };
    document.addEventListener('visibilitychange', visible);
    return () => { active = false; window.clearInterval(id); document.removeEventListener('visibilitychange', visible); };
  }, [taskId, refresh]);
  useEffect(() => {
    if (!trust?.active) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [trust?.active]);
  async function grant() {
    if (busy || !window.confirm(tr('Trust this conversation to edit approved workspace text files without individual approvals for the selected period? This can modify code and data.'))) return;
    setBusy(true); setError('');
    try { setTrust(await api.grantTaskWriteTrust(taskId,durationMinutes)); setNow(Date.now()); }
    catch (e) { setError(e instanceof Error ? e.message : tr('Unable to grant workspace write trust.')); }
    finally { setBusy(false); }
  }
  async function revoke() {
    if (busy) return;
    setBusy(true); setError('');
    try { setTrust(await api.revokeTaskWriteTrust(taskId)); setNow(Date.now()); }
    catch (e) { setError(e instanceof Error ? e.message : tr('Unable to revoke workspace write trust.')); }
    finally { setBusy(false); }
  }
  const active = trust?.active && trust.expiresAtMs !== null && trust.expiresAtMs > now && trust.usedCalls < trust.maxCalls;
  const seconds = active ? Math.max(0, Math.ceil(((trust?.expiresAtMs ?? 0) - now) / 1000)) : 0;
  return <section className="task-access-card" aria-label={tr('Trusted workspace edits')}>
    <header><FilePenLine aria-hidden="true" /><div>
      <h3>{tr('Trusted workspace edits')}</h3>
      <p>{tr('Optional: skip repeated approvals for bounded text edits in this conversation only.')}</p>
    </div></header>
    {active && trust ? <>
      <p role="status">{tr('Trusted edits: {used} / {max} calls', {used:trust.usedCalls,max:trust.maxCalls})}
        {' · '}{tr('Remaining: {count} calls', {count:Math.max(0, trust.maxCalls - trust.usedCalls)})}
        {' · '}{tr('Expires in {minutes} min', {minutes:Math.ceil(seconds / 60)})}</p>
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
    <button type="button" className="button secondary compact" disabled={busy} onClick={() => setRefresh(n => n+1)}>
      <RefreshCw /> {tr('Refresh trust status')}
    </button>
    {error && <p role="alert" className="task-access-error">{error}</p>}
  </section>;
}
