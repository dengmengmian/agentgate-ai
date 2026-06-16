import * as api from "@/lib/api";
import {
  normalizeModelsForProvider,
  pickModelsForProvider,
} from "@/lib/modelHeuristics";

export interface ProviderAutoSetupResult {
  models: string[];
  capabilitiesDetected: boolean;
}

export async function fetchDetectAndPersistProviderModels(
  providerId: string,
  providerType: string
): Promise<ProviderAutoSetupResult> {
  const fetchedModels = await api.fetchProviderModels(providerId);
  const models = normalizeModelsForProvider(providerType, fetchedModels);
  if (!models.length) {
    return { models, capabilitiesDetected: false };
  }

  const seeded = await api
    .seedModelCapabilities(providerType, models)
    .catch(() => null);
  const picked = pickModelsForProvider(providerType, models);

  await api.updateProvider(providerId, {
    supported_models: JSON.stringify(models),
    default_model: picked.default,
    reasoning_model: picked.reasoning,
    ...(seeded ? { model_capabilities: JSON.stringify(seeded) } : {}),
  });

  return { models, capabilitiesDetected: !!seeded };
}
