import { useEffect, useState } from 'react';
import { BookOpenCheck, RefreshCw } from 'lucide-react';
import { api, type TaskSkillRouting } from '../api';
import { tr } from '../i18n';

export function TaskSkillRoutingCard({ taskId }: { taskId: string }) {
  const [routing, setRouting] = useState<TaskSkillRouting | null>(null);
  const [error, setError] = useState('');
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    let alive = true;
    const load = () => {
      if (document.visibilityState === 'hidden') return;
      void api.taskSkillRouting(taskId).then(data => {
        if (alive) { setRouting(data); setError(''); }
      }).catch(() => { if (alive) setError(tr('Unable to load skill routing records.')); });
    };
    load();
    const timer = window.setInterval(load, 15000);
    document.addEventListener('visibilitychange', load);
    return () => {
      alive = false;
      window.clearInterval(timer);
      document.removeEventListener('visibilitychange', load);
    };
  }, [taskId, refresh]);
  const current = routing?.taskId === taskId ? routing : null;
  return <section className="task-access-card" aria-label={tr('Skill routing and use log')}>
    <header><BookOpenCheck aria-hidden="true" /><div>
      <h3>{tr('Skill routing and use log')}</h3>
      <p>{tr('Routing is based on task keywords; review matching instructions before use.')}</p>
    </div></header>
    <h4>{tr('Suggested for this task')}</h4>
    {current?.suggestions.length ? <ul className="skill-routing-items">
      {current.suggestions.map(item => <li key={item.skillId}>
        <div><strong>{item.title}</strong><small>{item.reason}</small></div>
        <span>{current.readIds.includes(item.skillId) ? tr('Skill read') : tr('Not read')}</span>
      </li>)}
    </ul> : <p>{tr('No skill recommendation for the latest turn.')}</p>}
    {current?.history.some(item => item.eventType === 'read') && <p>
      {tr('Observed skill reads')}: {current.history.filter(item => item.eventType === 'read')
        .slice(0, 5).map(item => item.skillId).filter(Boolean).join(', ')}
    </p>}
    <p className="task-access-feedback">
      {tr('Skill compliance is not automatically verified.')}{' '}
      {tr('Read records are observations, not proof of execution.')}
    </p>
    <button type="button" className="button secondary compact" onClick={() => setRefresh(v => v + 1)}>
      <RefreshCw />{tr('Refresh skill records')}
    </button>
    {error && <p role="alert" className="task-access-error">{error}</p>}
  </section>;
}
