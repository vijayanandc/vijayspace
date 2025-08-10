const { createApp } = Vue;

createApp({
  data() {
    return {
      modules: [],
      newModule: { name: '', label: '', slug: '', storageTable: 'MODULE_DATA_1' },
      selectedModule: null,
      fields: [],
      newField: { name: '', label: '', dataType: 'STRING', required: false },
      layouts: [],
      newLayout: { name: '', type: 'FORM', definitionJson: '{}' },
      recordPayload: '{}',
      createdRecord: null
    };
  },
  mounted() {
    this.loadModules();
  },
  methods: {
    async loadModules() {
      const res = await fetch('/api/v1/meta/modules');
      this.modules = await res.json();
    },
    async addModule() {
      await fetch('/api/v1/meta/modules', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(this.newModule)
      });
      this.newModule = { name: '', label: '', slug: '', storageTable: 'MODULE_DATA_1' };
      await this.loadModules();
    },
    selectModule(m) {
      this.selectedModule = m;
      this.loadFields();
      this.loadLayouts();
    },
    async loadFields() {
      const res = await fetch(`/api/v1/meta/modules/${this.selectedModule.slug}/fields`);
      this.fields = await res.json();
    },
    async addField() {
      await fetch(`/api/v1/meta/modules/${this.selectedModule.slug}/fields`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(this.newField)
      });
      this.newField = { name: '', label: '', dataType: 'STRING', required: false };
      await this.loadFields();
    },
    async loadLayouts() {
      const res = await fetch(`/api/v1/meta/modules/${this.selectedModule.slug}/layouts`);
      if (res.ok) {
        this.layouts = await res.json();
      }
    },
    async addLayout() {
      await fetch(`/api/v1/meta/modules/${this.selectedModule.slug}/layouts`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(this.newLayout)
      });
      this.newLayout = { name: '', type: 'FORM', definitionJson: '{}' };
      await this.loadLayouts();
    },
    async createRecord() {
      const res = await fetch(`/api/v1/data/${this.selectedModule.slug}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: this.recordPayload
      });
      if (res.ok) {
        this.createdRecord = await res.json();
        this.recordPayload = '{}';
      }
    }
  }
}).mount('#app');
