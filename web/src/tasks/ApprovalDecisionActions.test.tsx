import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApprovalDecisionActions } from './ApprovalDecisionActions';
import { highRiskApproval } from './highRiskApproval';

const resolveApproval = vi.fn();
vi.mock('../api', () => ({ api: { resolveTaskApproval: (...args: unknown[]) => resolveApproval(...args) } }));

describe('ApprovalDecisionActions', () => {
  const target = { taskId: 'task', activityId: 'activity', turnId: 'turn' };
  beforeEach(() => { resolveApproval.mockReset(); resolveApproval.mockResolvedValue({accepted:true}); });

  it('offers a reusable grant only for safe-read approvals', () => {
    const { rerender } = render(<ApprovalDecisionActions target={target} reusable />);
    expect(screen.getByRole('button', { name: /allow similar/i })).toBeInTheDocument();
    rerender(<ApprovalDecisionActions target={target} reusable={false} />);
    expect(screen.queryByRole('button', { name: /allow similar/i })).not.toBeInTheDocument();
  });

  it('does not allow one-click approval or reusable grants for destructive operations', async () => {
    render(<ApprovalDecisionActions target={target} reusable tool="fs_delete"
      summary={{ paths:['E:/repo/assets/a.bin'],pathCount:1, recursive:true, deleteMode:'permanent' }} />);
    expect(screen.getByRole('alert')).toHaveTextContent('Delete files or folders');
    expect(screen.queryByRole('button', {name:/allow similar/i})).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button',{name:/review risk before allowing/i}));
    const dialog=screen.getByRole('alertdialog');
    expect(dialog).toHaveTextContent('E:/repo/assets/a.bin');
    expect(dialog).toHaveTextContent('Includes nested folders');
    const confirm=screen.getByRole('button',{name:/confirm and allow once/i});
    expect(confirm).toBeDisabled();
    expect(resolveApproval).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('checkbox'));
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    await waitFor(() => expect(resolveApproval).toHaveBeenCalledWith('task','activity', {
      turnId:'turn', decision:'allow', reason:undefined, highRiskAcknowledged:true,
    }));
  });

  it('rejecting a high-risk request needs no acknowledgement', async () => {
    render(<ApprovalDecisionActions target={target} tool="fs_write_raw" summary={{paths:['C:/temp/blob.bin']}} />);
    fireEvent.click(screen.getByRole('button',{name:/^reject$/i}));
    fireEvent.click(screen.getAllByRole('button',{name:/^reject$/i}).at(-1)!);
    await waitFor(() => expect(resolveApproval).toHaveBeenCalledWith('task','activity', {
      turnId:'turn', decision:'reject', reason:undefined,
    }));
  });
});

describe('highRiskApproval', () => {
  it('classifies risky tools conservatively and keeps paths as literal text', () => {
    expect(highRiskApproval('fs_delete', {paths:['<script>alert(1)</script>']} )?.paths[0]).toBe('<script>alert(1)</script>');
    const move = highRiskApproval('fs_move', {paths:['source','destination'],overwrite:true,contentBytesEstimate:2048,dryRun:true});
    expect(move?.overwrite).toBe(true);
    expect(move?.source).toBe('source');
    expect(move?.destination).toBe('destination');
    expect(move?.contentBytesEstimate).toBe(2048);
    expect(move?.dryRun).toBe(true);
    expect(highRiskApproval('fs_write_raw', {paths:['a.bin']})).not.toBeNull();
    expect(highRiskApproval('fs_replace_text', {paths:['readme.md']})).toBeNull();
  });
});
