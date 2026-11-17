import { useMutation } from '@tanstack/react-query';

import { errorMessage } from '../../../api/client';
import { endpoints } from '../../../api/endpoints';
import type { JsonValue } from '../../../api/types';
import { Button } from '../../../components/Button';
import { ErrorBox } from '../../../components/Empty';
import { JsonView } from '../../../components/JsonView';
import { useBuilder } from '../BuilderContext';
import { updateStep } from '../model/tree';
import { testContext } from '../model/variables';

/** "Test step": runs the saved draft's action once with sample data; the output can become the sample. */
export function TestStepBox({ stepKey }: { stepKey: string }) {
  const { flowId, def, apply, flush, manifests, flow } = useBuilder();
  const run = useMutation({
    mutationFn: async () => {
      const saved = await flush();
      if (!saved) throw new Error('Save the flow before testing (see the save status at the top).');
      return endpoints.testStep(flowId, stepKey, testContext(def, stepKey, manifests, flow.trigger?.sample));
    },
  });
  const output: JsonValue | undefined = run.data?.output;
  return (
    <section className="panel-section">
      <h3>Test</h3>
      <p className="muted small">Calls the app once using sample data from earlier steps.</p>
      <Button onClick={() => run.mutate()} disabled={run.isPending}>
        {run.isPending ? 'Running…' : 'Test step'}
      </Button>
      {run.error && <ErrorBox message={errorMessage(run.error)} />}
      {run.data && (
        <div className="test-result">
          {run.data.error ? <ErrorBox message={`${run.data.error.code}: ${run.data.error.message}`} /> : <div className="ok-text">Success</div>}
          <JsonView label="Input" value={run.data.input} collapsed />
          {output !== undefined && (
            <>
              <JsonView label="Output" value={output} />
              <Button variant="primary" onClick={() => apply(updateStep(def, stepKey, (s) => ({ ...s, sample_output: output })))}>
                Use as sample
              </Button>
            </>
          )}
        </div>
      )}
    </section>
  );
}
