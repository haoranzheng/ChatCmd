import { useEffect, useState } from 'react';
import { LoaderCircle, Monitor, RefreshCw, ShieldAlert } from 'lucide-react';
import { api } from '../api';
import { tr } from '../i18n';

type Config = {enabled:boolean;port:number;endpoint:string};
type Status = {enabled:boolean;connected:boolean;tools:string[];error?:string};

/** Desktop automation must be deliberately enabled in authenticated local settings. */
export function DesktopIntegrationSettings() {
  const [config,setConfig]=useState<Config|null>(null);
  const [status,setStatus]=useState<Status|null>(null);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState('');
  const [notice,setNotice]=useState('');
  useEffect(()=>{
    let active=true;
    void api.desktopConfig().then((value)=>{if(active)setConfig(value);}).catch((reason:unknown)=>{
      if(active)setError(reason instanceof Error?reason.message:tr('Desktop settings unavailable'));
    });
    return ()=>{active=false;};
  },[]);
  const save=async()=>{
    if(!config||busy)return;
    setBusy(true);setError('');setNotice('');setStatus(null);
    try{
      if(config.enabled && !window.confirm(tr('Enable Windows desktop access? Every screenshot and GUI action requires local approval.')))return;
      const saved=await api.setDesktopConfig({enabled:config.enabled,port:config.port});
      setConfig(saved);
      setNotice(tr('Windows desktop configuration saved.'));
      if(saved.enabled)setStatus(await api.desktopStatus());
    }catch(reason){
      setError(reason instanceof Error?reason.message:tr('Could not save desktop configuration'));
    }finally{setBusy(false);}
  };
  const test=async()=>{
    if(!config||busy)return;
    setBusy(true);setError('');
    try{setStatus(await api.desktopStatus());}
    catch(reason){setError(reason instanceof Error?reason.message:tr('Connection test failed'));}
    finally{setBusy(false);}
  };
  return <div className="settings-section-block">
    <div className="settings-section-heading"><div><strong><Monitor />{tr('Windows Desktop Integration v0.1')}</strong><p>{tr('Connect to a local Windows-MCP server. Only the 127.0.0.1 loopback address is supported.')}</p></div></div>
    <div className="settings-control-grid one-column">
      <label className="settings-field-card"><span className="settings-field-copy"><strong>{tr('Enable Windows desktop tools')}</strong><small>{tr('Disabled by default. Each desktop action requires separate user approval, including Allow all mode.')}</small></span>
        <div className="settings-field-control"><input type="checkbox" checked={config?.enabled??false} disabled={!config||busy} onChange={(e)=>setConfig((old)=>old?{...old,enabled:e.target.checked}:old)}/></div>
      </label>
      <label className="settings-field-card"><span className="settings-field-copy"><strong>{tr('Windows-MCP local port')}</strong><small>{tr('Start Windows-MCP with Streamable HTTP bound to 127.0.0.1; default port 8000.')}</small></span>
        <div className="settings-field-control"><input type="number" min="1" max="65535" value={config?.port??8000} disabled={!config||busy} onChange={(e)=>setConfig((old)=>old?{...old,port:Number(e.target.value)}:old)}/></div>
      </label>
      {config && <p><small>http://127.0.0.1:{config.port}/mcp</small></p>}
      <p><ShieldAlert aria-hidden="true"/>{tr('Do not expose the Windows-MCP port publicly. Desktop tools are not a process sandbox; screenshots may reveal private content.')}</p>
      {status && <p role="status">{status.connected?tr('Connected to Windows-MCP'):tr('Not connected to Windows-MCP')}{status.error?` (${status.error})`:''}{status.tools.length?` — ${status.tools.join(', ')}`:''}</p>}
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <div className="settings-inline-actions">
        <button className="button primary" type="button" disabled={!config||busy||config.port<1||config.port>65535} onClick={()=>void save()}>{busy?<LoaderCircle className="spin"/>:<Monitor/>}{tr('Save desktop integration')}</button>
        <button className="button secondary" type="button" disabled={!config||busy||!status&&!config.enabled} onClick={()=>void test()}><RefreshCw/>{tr('Test Windows-MCP connection')}</button>
      </div>
    </div>
  </div>;
}
