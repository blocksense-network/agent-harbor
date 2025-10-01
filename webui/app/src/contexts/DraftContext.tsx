import {
  createContext,
  useContext,
  Component,
  JSX,
  createSignal,
} from "solid-js";
import { apiClient, DraftTask } from "../lib/api";

// DraftContext provides CRUD operations for drafts
// Draft data itself comes from route-level fetching via props (progressive enhancement)
interface DraftContextValue {
  error: () => string | null;
  createDraft: (draft: Partial<DraftTask>) => Promise<DraftTask | null>;
  removeDraft: (id: string) => Promise<boolean>;
  updateDraft: (id: string, updates: Partial<DraftTask>) => Promise<boolean>;
  onDraftChanged?: () => void; // Callback for components to refetch after changes
}

const DraftContext = createContext<DraftContextValue>({
  error: () => null,
  createDraft: async () => null,
  removeDraft: async () => false,
  updateDraft: async () => false,
});

interface DraftProviderProps {
  children: JSX.Element;
  onDraftChanged?: () => void; // Optional callback when drafts change
}

export const DraftProvider: Component<DraftProviderProps> = (props) => {
  const [error, setError] = createSignal<string | null>(null);

  const createDraft = async (
    draft: Partial<DraftTask>,
  ): Promise<DraftTask | null> => {
    try {
      setError(null);
      console.log("[DraftContext] Creating draft...", draft);
      const response = await apiClient.createDraft(draft);
      console.log("[DraftContext] Draft created (response):", response);

      // API returns only { id, createdAt, updatedAt }, construct full draft
      const fullDraft: DraftTask = {
        ...(draft as DraftTask),
        id: response.id,
        createdAt: response.createdAt,
        updatedAt: response.updatedAt,
      };
      console.log("[DraftContext] Full draft:", fullDraft);

      props.onDraftChanged?.(); // Notify that drafts changed

      // Dispatch custom event for components to listen to
      if (typeof window !== "undefined") {
        console.log("[DraftContext] Dispatching draft-created event");
        window.dispatchEvent(
          new CustomEvent("draft-created", { detail: fullDraft }),
        );
      }

      return fullDraft;
    } catch (err) {
      const errorMessage =
        err instanceof Error ? err.message : "Failed to create draft";
      setError(errorMessage);
      console.error("Failed to create draft:", err);
      return null;
    }
  };

  const removeDraft = async (id: string): Promise<boolean> => {
    try {
      setError(null);
      await apiClient.deleteDraft(id);
      props.onDraftChanged?.(); // Notify that drafts changed
      return true;
    } catch (err) {
      const errorMessage =
        err instanceof Error ? err.message : "Failed to delete draft";
      setError(errorMessage);
      console.error("Failed to delete draft:", err);
      return false;
    }
  };

  const updateDraft = async (
    id: string,
    updates: Partial<DraftTask>,
  ): Promise<boolean> => {
    try {
      setError(null);
      // Remove server-managed fields from updates
      const { id: _, createdAt, updatedAt, ...updateData } = updates as any;
      await apiClient.updateDraft(id, updateData);
      props.onDraftChanged?.(); // Notify that drafts changed
      return true;
    } catch (err) {
      const errorMessage =
        err instanceof Error ? err.message : "Failed to update draft";
      setError(errorMessage);
      console.error(`Failed to update draft:`, err);
      return false;
    }
  };

  const value: DraftContextValue = {
    error,
    createDraft,
    removeDraft,
    updateDraft,
    onDraftChanged: props.onDraftChanged,
  };

  return (
    <DraftContext.Provider value={value}>
      {props.children}
    </DraftContext.Provider>
  );
};

export const useDrafts = () => {
  const context = useContext(DraftContext);
  if (!context) {
    throw new Error("useDrafts must be used within a DraftProvider");
  }
  return context;
};
