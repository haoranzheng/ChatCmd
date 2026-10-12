import { AlertTriangle, Ban, Check, CheckCheck, CircleAlert, LoaderCircle, X } from 'lucide-react';
import { FormEvent, useEffect, useId, useRef, useState } from 'react';
import { api } from '../api';
import { Modal } from '../components';
import { tr } from '../i18n';
import { highRiskApproval, type HighRiskApproval } from './highRiskApproval';

export interface ApprovalDecisionTarget { taskId: string; activityId: string; turnId?: string; }

function HighRiskWarning({ risk }: { risk: HighRiskApproval }) {
  return <div className="approval-high-risk-warning" role="alert">
    <div className="approval-high-risk-heading"><AlertTriangle aria-hidden="true" /><strong>{tr('High-risk action: {action}', { action: risk.title })}</strong></div>
    <p>{risk.consequence}</p>
    {risk.paths.length > 0
      ? <div className="approval-high-risk-paths"><strong>{tr('Affected paths')}</strong>
        <ul>{risk.paths.map((path, index) => <li key={index}><code title={path}>{path}</code></li>)}</ul>
        {risk.pathCount > risk.paths.length && <small>{tr('Plus {count} additional paths', { count: risk.pathCount - risk.paths.length })}</small>}
      </div>
      : <p className="approval-high-risk-no-path">{tr('Target details are not available. Inspect the request before approval.')}</p>}
    <div className="approval-high-risk-flags">
      {risk.recursive && <span>{tr('Includes nested folders')}</span>}
      {risk.overwrite && <span>{tr('May overwrite existing data')}</span>}
      {risk.mode && <span>{tr('Deletion mode: {mode}', { mode: risk.mode })}</span>}
    </div>
  </div>;
}

export function ApprovalDecisionActions({ target, onResolved, reusable = true, tool, summary }: {
  target: ApprovalDecisionTarget;
  onResolved?: () => void;
  reusable?: boolean;
  tool?: string;
  summary?: unknown;
}) {
  const [busy, setBusy] = useState<'allow' | 'allowSimilar' | 'reject' | null>(null);
  const [error, setError] = useState('');
  const [rejecting, setRejecting] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [understood, setUnderstood] = useState(false);
  const risk = highRiskApproval(tool, summary);
  const resolve = async (decision: 'allow' | 'allowSimilar' | 'reject', reason?: string, highRiskAcknowledged = false) => {
    if (busy) return;
    setBusy(decision); setError('');
    try {
      await api.resolveTaskApproval(target.taskId, target.activityId, {
        turnId: target.turnId, decision, reason, ...(highRiskAcknowledged ? { highRiskAcknowledged: true } : {}),
      });
      setRejecting(false);
      setConfirming(false);
      onResolved?.();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : tr('Could not send approval decision.'));
      setBusy(null);
    }
  };
  const closeConfirm = () => { if (!busy) { setConfirming(false); setUnderstood(false); } };
  return <>
    {risk && <div className="approval-high-risk-inline"><AlertTriangle aria-hidden="true" /><div>
      <strong>{tr('High-risk action: {action}', { action: risk.title })}</strong>
      <p>{tr('Approval requires reviewing the affected files and confirming the consequences.')}</p>
    </div></div>}
    <div className="approval-actions" role="group" aria-label={tr('Approval decision')}>
      <button type="button" className={risk ? 'approval-risk-review' : 'approval-allow'} disabled={Boolean(busy)}
        onClick={() => { if (risk) { setUnderstood(false); setConfirming(true); } else void resolve('allow'); }}>
        {busy === 'allow' ? <LoaderCircle className="spin" /> : risk ? <AlertTriangle /> : <Check />}
        {risk ? tr('Review risk before allowing') : tr('Allow')}
      </button>
      {!risk && reusable && <button type="button" className="approval-similar" disabled={Boolean(busy)}
        onClick={() => void resolve('allowSimilar')}>
        {busy === 'allowSimilar' ? <LoaderCircle className="spin" /> : <CheckCheck />}{tr('Allow similar')}
      </button>}
      <button type="button" className="approval-reject" disabled={Boolean(busy)} onClick={() => setRejecting(true)}><Ban />{tr('Reject')}</button>
      {error && <p role="alert"><CircleAlert />{error}</p>}
    </div>
    {risk && confirming && <Modal title={tr('Confirm high-risk operation')} description={tr('This approval applies to one operation only. Verify the target and consequences.')} close={closeConfirm} dangerous dismissible={false} className="approval-risk-modal">
      <HighRiskWarning risk={risk} />
      <label className="approval-risk-acknowledge"><input type="checkbox" checked={understood} disabled={Boolean(busy)}
        onChange={event => setUnderstood(event.target.checked)} />
        <span>{tr('I have reviewed the target and understand this operation may lose or overwrite data.')}</span>
      </label>
      {error && <p role="alert" className="approval-risk-error">{error}</p>}
      <div className="modal-actions">
        <button type="button" className="button secondary" disabled={Boolean(busy)} onClick={closeConfirm}>{tr('Cancel')}</button>
        <button type="button" className="button danger" disabled={!understood || Boolean(busy)}
          onClick={() => void resolve('allow', undefined, true)}>
          {busy === 'allow' ? <LoaderCircle className="spin" /> : <AlertTriangle />} {tr('Confirm and allow once')}
        </button>
      </div>
    </Modal>}
    {rejecting && <RejectApprovalDialog busy={busy === 'reject'} error={error} close={() => !busy && setRejecting(false)} reject={(reason) => resolve('reject', reason)} />}
  </>;
}

function RejectApprovalDialog({ busy, error, close, reject }: { busy: boolean; error: string; close: () => void; reject: (reason?: string) => Promise<void> }) {
  const [reason, setReason] = useState(''); const titleId = useId(); const descriptionId = useId(); const dialogRef = useRef<HTMLDivElement>(null); const reasonRef = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null; reasonRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === 'Escape' && !busy) { event.preventDefault(); close(); return; } if (event.key !== 'Tab') return; const focusable = [...dialogRef.current?.querySelectorAll<HTMLElement>('button:not(:disabled), textarea:not(:disabled)') ?? []]; if (!focusable.length) return; const first = focusable[0]; const last = focusable.at(-1)!; if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); } else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); } };
    document.addEventListener('keydown', onKeyDown); return () => { document.removeEventListener('keydown', onKeyDown); previous?.focus(); };
  }, [busy, close]);
  const submit = (event: FormEvent) => { event.preventDefault(); if (!busy) void reject(reason.trim() || undefined); };
  return <div className="modal-backdrop approval-reject-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) close(); }}><form onSubmit={submit}><div ref={dialogRef} className="modal compact-modal approval-reject-modal" role="alertdialog" aria-modal="true" aria-labelledby={titleId} aria-describedby={descriptionId} aria-busy={busy}>
    <header><div><h2 id={titleId}>{tr('Reject permission request?')}</h2><p id={descriptionId}>{tr('The reason will be sent back to the Agent so it knows why the command was not allowed.')}</p></div><button type="button" className="icon-button" aria-label={tr('Close')} disabled={busy} onClick={close}><X /></button></header>
    <label htmlFor={`${titleId}-reason`}>{tr('Rejection reason (optional)')}</label><textarea ref={reasonRef} id={`${titleId}-reason`} value={reason} maxLength={2000} rows={4} disabled={busy} placeholder={tr('Example: Do not run this command; use a read-only approach instead.')} onChange={(event) => setReason(event.target.value)} />
    <div className="stop-reason-meta"><span>{tr('This reason will be sent back to the AI.')}</span><span>{reason.length}/2000</span></div>{error && <p className="stop-activity-error" role="alert"><CircleAlert />{error}</p>}
    <div className="modal-actions"><button type="button" className="button secondary" disabled={busy} onClick={close}>{tr('Cancel')}</button><button type="submit" className="button danger" disabled={busy}>{busy ? <LoaderCircle className="spin" /> : <Ban />}{busy ? tr('Rejecting…') : tr('Reject')}</button></div>
  </div></form></div>;
}
