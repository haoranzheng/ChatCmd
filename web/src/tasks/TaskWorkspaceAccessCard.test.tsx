import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { api } from '../api';
import { TaskWorkspaceAccessCard } from './TaskWorkspaceAccessCard';

vi.mock('../api', () => ({
  api: {
    workspaceProjects: vi.fn(),
    taskWorkspace: vi.fn(),
    setTaskWorkspace: vi.fn(),
  },
}));

const initial = { taskId: 'task-1', projectId: null, projectFolder: null, accessMode: 'readOnly' as const, locallyAuthorized: false };
const project = { id: 'project-1', name: 'Temporary project', path: '/temp/project', pathValid: true };

describe('TaskWorkspaceAccessCard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.workspaceProjects).mockResolvedValue([project]);
    vi.mocked(api.taskWorkspace).mockResolvedValue(initial);
    vi.mocked(api.setTaskWorkspace).mockResolvedValue({
      taskId: initial.taskId,
      projectId: project.id,
      projectFolder: project.path,
      accessMode: 'readWrite',
      locallyAuthorized: true,
    });
  });

  it('starts unbound and never grants a project without a local selection', async () => {
    render(<TaskWorkspaceAccessCard taskId="task-1" onBound={vi.fn()} />);
    await screen.findByText(/no locally approved workspace binding/i);
    expect(screen.getByRole('button', { name: /save workspace access/i })).toBeDisabled();
    expect(api.setTaskWorkspace).not.toHaveBeenCalled();
  });

  it('binds read-write only after the local confirmation and can later revoke', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const onBound = vi.fn();
    render(<TaskWorkspaceAccessCard taskId="task-1" onBound={onBound} />);
    await screen.findByText(/no locally approved workspace binding/i);
    fireEvent.change(screen.getByLabelText('Workspace project'), { target: { value: 'project-1' } });
    fireEvent.change(screen.getByLabelText('File access mode'), { target: { value: 'readWrite' } });
    fireEvent.click(screen.getByRole('button', { name: /save workspace access/i }));
    await waitFor(() => expect(api.setTaskWorkspace).toHaveBeenCalledWith('task-1', { projectId: 'project-1', accessMode: 'readWrite' }));
    expect(confirm).toHaveBeenCalled();
    expect(onBound).toHaveBeenCalledWith('/temp/project');

    fireEvent.change(screen.getByLabelText('File access mode'), { target: { value: 'restricted' } });
    fireEvent.click(screen.getByRole('button', { name: /save workspace access/i }));
    await waitFor(() => expect(api.setTaskWorkspace).toHaveBeenLastCalledWith('task-1', { projectId: 'project-1', accessMode: 'restricted' }));
    confirm.mockRestore();
  });

  it('refuses to bind a missing or unsafe project folder', async () => {
    vi.mocked(api.workspaceProjects).mockResolvedValue([{ ...project, pathValid: false }]);
    render(<TaskWorkspaceAccessCard taskId="task-1" onBound={vi.fn()} />);
    await screen.findByText(/no locally approved workspace binding/i);
    fireEvent.change(screen.getByLabelText('Workspace project'), { target: { value: 'project-1' } });
    expect(screen.getByRole('button', { name: /save workspace access/i })).toBeDisabled();
    expect(api.setTaskWorkspace).not.toHaveBeenCalled();
  });
});
