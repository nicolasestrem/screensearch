import { useState } from 'react';
import { Plus, Edit2, Trash2, X, Check } from 'lucide-react';
import { useTags, useCreateTag, useUpdateTag, useDeleteTag } from '../hooks/useTags';
import { Tag } from '../types';

export function TagManager() {
  const { data: tags = [], isLoading } = useTags();
  const createTag = useCreateTag();
  const updateTag = useUpdateTag();
  const deleteTag = useDeleteTag();

  const [isCreating, setIsCreating] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [formData, setFormData] = useState({ name: '', color: '#3b82f6' });

  const handleCreate = async () => {
    if (!formData.name.trim()) return;

    await createTag.mutateAsync(formData);
    setIsCreating(false);
    setFormData({ name: '', color: '#3b82f6' });
  };

  const handleUpdate = async (id: number) => {
    if (!formData.name.trim()) return;

    await updateTag.mutateAsync({ id, tag: formData });
    setEditingId(null);
    setFormData({ name: '', color: '#3b82f6' });
  };

  const handleDelete = async (id: number) => {
    if (confirm('Are you sure you want to delete this tag?')) {
      await deleteTag.mutateAsync(id);
    }
  };

  const startEditing = (tag: Tag) => {
    setEditingId(tag.id);
    setFormData({ name: tag.name, color: tag.color || '#3b82f6' });
  };

  const cancelEditing = () => {
    setEditingId(null);
    setIsCreating(false);
    setFormData({ name: '', color: '#3b82f6' });
  };

  if (isLoading) {
    return <div className="text-center py-4">Loading tags...</div>;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-semibold">Tags</h3>
        {!isCreating && (
          <button
            onClick={() => setIsCreating(true)}
            className="flex items-center gap-2 px-3 py-1.5 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
          >
            <Plus className="h-4 w-4" />
            <span>New Tag</span>
          </button>
        )}
      </div>

      {/* Create Form */}
      {isCreating && (
        <div className="bg-card border border-border rounded-lg p-4 space-y-3 animate-slide-in">
          <div className="space-y-1">
            <input
              type="text"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              placeholder="Tag name"
              maxLength={200}
              className="w-full px-3 py-2 bg-background border border-input rounded-md"
              autoFocus
            />
            <p className="text-xs text-muted-foreground">
              {formData.name.length}/200 characters
            </p>
          </div>
          <div className="flex items-center gap-2">
            <label className="text-sm font-medium">Color:</label>
            <input
              type="color"
              value={formData.color}
              onChange={(e) => setFormData({ ...formData, color: e.target.value })}
              className="w-12 h-8 rounded cursor-pointer"
            />
            <span className="text-sm text-muted-foreground">{formData.color}</span>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={handleCreate}
              disabled={createTag.isPending}
              className="flex items-center gap-2 px-3 py-1.5 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 transition-colors"
            >
              <Check className="h-4 w-4" />
              <span>Create</span>
            </button>
            <button
              onClick={cancelEditing}
              className="flex items-center gap-2 px-3 py-1.5 bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/80 transition-colors"
            >
              <X className="h-4 w-4" />
              <span>Cancel</span>
            </button>
          </div>
        </div>
      )}

      {/* Tags List */}
      <div className="space-y-2">
        {tags.map((tag) => (
          <div
            key={tag.id}
            className="bg-card border border-border rounded-lg p-4 hover:shadow-md transition-shadow"
          >
            {editingId === tag.id ? (
              /* Edit Form */
              <div className="space-y-3">
                <input
                  type="text"
                  value={formData.name}
                  onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                  className="w-full px-3 py-2 bg-background border border-input rounded-md"
                  autoFocus
                />
                <div className="flex items-center gap-2">
                  <label className="text-sm font-medium">Color:</label>
                  <input
                    type="color"
                    value={formData.color}
                    onChange={(e) =>
                      setFormData({ ...formData, color: e.target.value })
                    }
                    className="w-12 h-8 rounded cursor-pointer"
                  />
                  <span className="text-sm text-muted-foreground">{formData.color}</span>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => handleUpdate(tag.id)}
                    disabled={updateTag.isPending}
                    className="flex items-center gap-2 px-3 py-1.5 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 transition-colors"
                  >
                    <Check className="h-4 w-4" />
                    <span>Save</span>
                  </button>
                  <button
                    onClick={cancelEditing}
                    className="flex items-center gap-2 px-3 py-1.5 bg-secondary text-secondary-foreground rounded-lg hover:bg-secondary/80 transition-colors"
                  >
                    <X className="h-4 w-4" />
                    <span>Cancel</span>
                  </button>
                </div>
              </div>
            ) : (
              /* Tag Display */
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div
                    className="w-6 h-6 rounded-full"
                    style={{ backgroundColor: tag.color || '#888' }}
                  />
                  <span className="font-medium">{tag.name}</span>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => startEditing(tag)}
                    className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
                  >
                    <Edit2 className="h-4 w-4" />
                  </button>
                  <button
                    onClick={() => handleDelete(tag.id)}
                    className="p-2 text-muted-foreground hover:text-destructive hover:bg-accent rounded-lg transition-colors"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>
            )}
          </div>
        ))}

        {tags.length === 0 && !isCreating && (
          <div className="text-center py-8 text-muted-foreground">
            <p>No tags yet</p>
            <p className="text-sm">Create your first tag to organize your captures</p>
          </div>
        )}
      </div>
    </div>
  );
}
