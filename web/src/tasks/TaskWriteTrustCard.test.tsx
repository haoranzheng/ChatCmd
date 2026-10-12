import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TaskWriteTrustCard } from './TaskWriteTrustCard';

const taskWriteTrust = vi.fn();
const grantTaskWriteTrust = vi.fn();
const revokeTaskWriteTrust = vi.fn();

vi.mock('../api', () => ({ api: { taskWriteTrust: (...args: unknown[]) => taskWriteTrust(...args),
  grantTaskWriteTrust: (...args: unknown[]) => grantTaskWriteTrust(...args),
  revokeTaskWriteTrust: (...args: unknown[]) => revokeTaskWriteTrust(...args) } }));

describe('TaskWriteTrustCard', () => {
  beforeEach(() => {
    taskWriteTrust.mockReset();
    grantTaskWriteTrust.mockReset();
    revokeTaskWriteTrust.mockReset();
  });

  it('displays live usage and refreshes without renewing permission', async () => {
    taskWriteTrust.mockResolvedValueOnce({taskId:'task',active:true,expiresAtMs:Date.now()+600000,maxCalls:128,usedCalls:3})
      .mockResolvedValue({taskId:'task',active:true,expiresAtMs:Date.now()+600000,maxCalls:128,usedCalls:9});
    render(<TaskWriteTrustCard taskId="task" />);
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('3 / 128'));
    expect(screen.getByRole('status')).toHaveTextContent('Remaining: 125 calls');
    fireEvent.click(screen.getByRole('button',{name:/refresh trust status/i}));
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('9 / 128'));
    expect(grantTaskWriteTrust).not.toHaveBeenCalled();
    expect(revokeTaskWriteTrust).not.toHaveBeenCalled();
  });

  it('does not offer an active trust after it expires', async () => {
    taskWriteTrust.mockResolvedValue({taskId:'task',active:true,expiresAtMs:1,maxCalls:128,usedCalls:1});
    render(<TaskWriteTrustCard taskId="task" />);
    await waitFor(() => expect(screen.getByRole('button',{name:/trust text edits/i})).toBeEnabled());
    expect(screen.queryByRole('button',{name:/revoke write trust/i})).not.toBeInTheDocument();
  });
});
