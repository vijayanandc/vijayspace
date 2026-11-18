// "Test" tab of an action: runs the saved draft's action once with sample data; the output can become the sample.
// Custom functions also show their `print` lines as a log and script errors with their line and column.

import { useMutation } from '@tanstack/react-query';
import { FlaskConical } from 'lucide-react';

import { errorMessage } from '../../../api/client';
import { endpoints } from '../../../api/endpoints';
import type { JsonValue, StepTestResult } from '../../../api/types';
import { Button } from '../../../components/Button';
import { ErrorBox } from '../../../components/Empty';
import { JsonView } from '../../../components/JsonView';
import { LogBlock } from '../../../components/LogBlock';
import { MessageBar } from '../../../components/MessageBar';
import { useBuilder } from '../BuilderContext';
import { formatLocation } from '../model/code';
import { isScriptStep, scriptError, splitLogs } from '../model/script';
import { findStep, updateStep } from '../model/tree';
import { testContext } from '../model/variables';

function ErrorResult({ error }: { error: NonNullable<StepTestResult['error']> }) {
  const script = scriptError(error);
  if (!script) {
    return (
      <MessageBar intent="error" title={error.code}>
        {error.message}
      </MessageBar>
    );
  }
  return (
    <MessageBar intent="error" title={script.code === 'script.limit_exceeded' ? 'Script limit exceeded' : 'Script error'}>
      {formatLocation(script.line, script.column, script.text)} <span className="mono muted">({script.code})</span>
    </MessageBar>
  );
}

export function TestStepBox({ stepKey }: { stepKey: string }) {
  const { flowId, def, apply, flush, manifests, flow } = useBuilder();
  const step = findStep(def, stepKey);
  const script = step ? isScriptStep(step) : false;
  const run = useMutation({
    mutationFn: async () => {
      const saved = await flush();
      if (!saved) throw new Error('Save the flow before testing (see the save status at the top).');
      return endpoints.testStep(flowId, stepKey, testContext(def, stepKey, manifests, flow.trigger?.sample));
    },
  });
  const output: JsonValue | undefined = run.data?.output;
  const { logs, rest } = splitLogs(output);
  return (
    <div className="panel-body">
      <section className="panel-section">
        <h3 className="section-title">Test this step</h3>
        <p className="field-help">
          {script
            ? 'Runs the script once with sample data from earlier steps. Scripts are sandboxed, so nothing outside Weft changes.'
            : 'Calls the app once using sample data from earlier steps. This really runs the action.'}
        </p>
        <Button variant="primary" icon={FlaskConical} onClick={() => run.mutate()} disabled={run.isPending}>
          {run.isPending ? 'Running…' : 'Test step'}
        </Button>
        {run.error && <ErrorBox message={errorMessage(run.error)} />}
      </section>
      {run.data && (
        <section className="panel-section test-result">
          <h3 className="section-title">Result</h3>
          {run.data.error ? <ErrorResult error={run.data.error} /> : <MessageBar intent="success">The step ran successfully.</MessageBar>}
          <JsonView label="Input" value={run.data.input} collapsed />
          {output !== undefined && (
            <>
              <JsonView label="Output" value={rest} />
              <LogBlock lines={logs} />
              <Button onClick={() => apply(updateStep(def, stepKey, (s) => ({ ...s, sample_output: output })))}>Use as sample</Button>
            </>
          )}
        </section>
      )}
    </div>
  );
}
