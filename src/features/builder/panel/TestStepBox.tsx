// "Test" tab of an action: runs the saved draft's action once with sample data; the output can become the sample.

import { useMutation } from '@tanstack/react-query';
import { FlaskConical } from 'lucide-react';

import { errorMessage } from '../../../api/client';
import { endpoints } from '../../../api/endpoints';
import type { JsonValue } from '../../../api/types';
import { Button } from '../../../components/Button';
import { ErrorBox } from '../../../components/Empty';
import { JsonView } from '../../../components/JsonView';
import { MessageBar } from '../../../components/MessageBar';
import { useBuilder } from '../BuilderContext';
import { updateStep } from '../model/tree';
import { testContext } from '../model/variables';

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
    <div className="panel-body">
      <section className="panel-section">
        <h3 className="section-title">Test this step</h3>
        <p className="field-help">Calls the app once using sample data from earlier steps. This really runs the action.</p>
        <Button variant="primary" icon={FlaskConical} onClick={() => run.mutate()} disabled={run.isPending}>
          {run.isPending ? 'Running…' : 'Test step'}
        </Button>
        {run.error && <ErrorBox message={errorMessage(run.error)} />}
      </section>
      {run.data && (
        <section className="panel-section test-result">
          <h3 className="section-title">Result</h3>
          {run.data.error ? (
            <MessageBar intent="error" title={run.data.error.code}>
              {run.data.error.message}
            </MessageBar>
          ) : (
            <MessageBar intent="success">The step ran successfully.</MessageBar>
          )}
          <JsonView label="Input" value={run.data.input} collapsed />
          {output !== undefined && (
            <>
              <JsonView label="Output" value={output} />
              <Button onClick={() => apply(updateStep(def, stepKey, (s) => ({ ...s, sample_output: output })))}>Use as sample</Button>
            </>
          )}
        </section>
      )}
    </div>
  );
}
